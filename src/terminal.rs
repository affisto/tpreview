//! A single /dev/tty reader owns keyboard bytes and terminal replies. No probe reads stdin.
use crate::{cli::RenderMode, source::safe};
use crossterm::{
    cursor,
    event::{KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{
    collections::VecDeque,
    fs::{File, OpenOptions},
    io::{self, IsTerminal, Read, Write},
    os::fd::AsRawFd,
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub enum InputEvent {
    Key(KeyEvent),
    Reply(String),
}
#[derive(Default)]
pub struct Decoder {
    buffer: Vec<u8>,
    escape_at: Option<Instant>,
    paste: bool,
}
impl Decoder {
    pub fn feed(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
        if self.buffer.first() == Some(&27) && self.escape_at.is_none() {
            self.escape_at = Some(Instant::now());
        }
    }
    pub fn next_event(&mut self) -> Option<InputEvent> {
        loop {
            if self.buffer.is_empty() {
                self.escape_at = None;
                return None;
            }
            if self.paste {
                if let Some(end) = self.buffer.windows(6).position(|w| w == b"\x1b[201~") {
                    self.buffer.drain(..end + 6);
                    self.paste = false;
                    continue;
                }
                if self.buffer.len() > 6 {
                    self.buffer.drain(..self.buffer.len() - 6);
                }
                return None;
            }
            if self.buffer.starts_with(b"\x1b[200~") {
                self.buffer.drain(..6);
                self.paste = true;
                continue;
            }
            let mut modifiers = KeyModifiers::NONE;
            let (n, code) = match self.buffer[0] {
                27 => {
                    if self.buffer.len() == 1 {
                        if self
                            .escape_at
                            .is_some_and(|t| t.elapsed() < Duration::from_millis(35))
                        {
                            return None;
                        }
                        (1, KeyCode::Esc)
                    } else if matches!(self.buffer[1], b'_' | b']' | b'P' | b'^') {
                        // APC/OSC/DCS replies, including late replies, never become keyboard input.
                        if let Some(end) = self.buffer.windows(2).position(|w| w == b"\x1b\\") {
                            let s = String::from_utf8_lossy(&self.buffer[..end + 2]).into_owned();
                            self.buffer.drain(..end + 2);
                            return Some(InputEvent::Reply(s));
                        }
                        if let Some(end) = self.buffer.iter().position(|b| *b == 7) {
                            self.buffer.drain(..end + 1);
                            continue;
                        }
                        if self.buffer.len() > 8192 {
                            self.buffer.clear();
                        }
                        return None;
                    } else if self.buffer[1] == b'[' || self.buffer[1] == b'O' {
                        let end = self
                            .buffer
                            .iter()
                            .enumerate()
                            .skip(2)
                            .find(|(_, b)| (0x40..=0x7e).contains(*b))
                            .map(|(i, _)| i);
                        let Some(end) = end else {
                            if self.buffer.len() > 128 {
                                self.buffer.clear();
                            }
                            return None;
                        };
                        let seq = &self.buffer[2..=end];
                        let code = match seq {
                            b"A" => Some(KeyCode::Up),
                            b"B" => Some(KeyCode::Down),
                            b"C" => Some(KeyCode::Right),
                            b"D" => Some(KeyCode::Left),
                            b"H" | b"1~" | b"7~" => Some(KeyCode::Home),
                            b"F" | b"4~" | b"8~" => Some(KeyCode::End),
                            b"5~" => Some(KeyCode::PageUp),
                            b"6~" => Some(KeyCode::PageDown),
                            b"Z" => Some(KeyCode::BackTab),
                            _ => None,
                        };
                        if let Some(code) = code {
                            (end + 1, code)
                        } else {
                            let s = String::from_utf8_lossy(&self.buffer[..end + 1]).into_owned();
                            self.buffer.drain(..end + 1);
                            return Some(InputEvent::Reply(s));
                        }
                    } else {
                        (1, KeyCode::Esc)
                    }
                }
                3 => {
                    modifiers = KeyModifiers::CONTROL;
                    (1, KeyCode::Char('c'))
                }
                9 => (1, KeyCode::Tab),
                10 | 13 => (1, KeyCode::Enter),
                8 | 127 => (1, KeyCode::Backspace),
                b if b < 32 => {
                    self.buffer.remove(0);
                    continue;
                }
                _ => {
                    let n = match self.buffer[0] {
                        0..=0x7f => 1,
                        0xc2..=0xdf => 2,
                        0xe0..=0xef => 3,
                        0xf0..=0xf4 => 4,
                        _ => {
                            self.buffer.remove(0);
                            continue;
                        }
                    };
                    if self.buffer.len() < n {
                        return None;
                    }
                    let c = std::str::from_utf8(&self.buffer[..n])
                        .ok()
                        .and_then(|s| s.chars().next())
                        .unwrap_or('�');
                    (n, KeyCode::Char(c))
                }
            };
            self.buffer.drain(..n);
            self.escape_at = None;
            return Some(InputEvent::Key(KeyEvent::new(code, modifiers)));
        }
    }
}
pub struct Input {
    file: File,
    decoder: Decoder,
    pub pending: VecDeque<KeyEvent>,
}
impl Input {
    pub fn open() -> io::Result<Self> {
        Ok(Self {
            file: OpenOptions::new().read(true).write(true).open("/dev/tty")?,
            decoder: Decoder::default(),
            pending: VecDeque::new(),
        })
    }
    pub fn poll(&mut self, timeout: Duration) -> io::Result<Option<InputEvent>> {
        if let Some(ev) = self.decoder.next_event() {
            return Ok(Some(ev));
        }
        // select works with /dev/tty on both Darwin and Linux (Darwin's tty alias
        // does not reliably report read readiness through poll).
        let mut readfds = unsafe { std::mem::zeroed::<libc::fd_set>() };
        let fd = self.file.as_raw_fd();
        if fd >= libc::FD_SETSIZE as i32 {
            return Err(io::Error::other("TTY descriptor exceeds select limit"));
        }
        let mut wait = libc::timeval {
            tv_sec: timeout.as_secs() as _,
            tv_usec: timeout.subsec_micros() as _,
        };
        let result = unsafe {
            libc::FD_ZERO(&mut readfds);
            libc::FD_SET(fd, &mut readfds);
            libc::select(
                fd + 1,
                &mut readfds,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut wait,
            )
        };
        if result < 0 {
            let e = io::Error::last_os_error();
            if e.kind() != io::ErrorKind::Interrupted {
                return Err(e);
            }
        }
        if result > 0 {
            let mut b = [0; 4096];
            let n = self.file.read(&mut b)?;
            self.decoder.feed(&b[..n]);
        }
        Ok(self.decoder.next_event())
    }
}
#[derive(Debug, Clone)]
pub struct Capabilities {
    pub image: bool,
    pub cell: (u16, u16),
    pub reason: String,
    pub tmux: bool,
}
impl Capabilities {
    pub fn detect(input: Option<&mut Input>, mode: RenderMode) -> io::Result<Self> {
        let tmux = std::env::var_os("TMUX").is_some();
        let mut cap = Self {
            image: false,
            cell: (8, 16),
            reason: String::new(),
            tmux,
        };
        if !io::stdout().is_terminal() {
            cap.reason = "stdout is not a TTY; no capability queries emitted".into();
            return Ok(cap);
        }
        if tmux {
            cap.reason="tmux: image placements/pane clipping not verified; safe text fallback (settings unchanged)".into();
            return Ok(cap);
        }
        if std::env::var("TERM").is_ok_and(|s| s.starts_with("screen")) {
            cap.reason = "screen/multiplexer: safe text fallback".into();
            return Ok(cap);
        }
        if mode == RenderMode::Text {
            cap.reason = "text requested; probe skipped".into();
            return Ok(cap);
        }
        let Some(input) = input else {
            cap.reason = "no controlling TTY; text fallback".into();
            return Ok(cap);
        };
        input
            .file
            .write_all(b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[16t")?;
        input.file.flush()?;
        let deadline = Instant::now() + Duration::from_millis(350);
        let mut kitty = false;
        let mut cell = false;
        while Instant::now() < deadline {
            match input.poll(Duration::from_millis(15))? {
                Some(InputEvent::Reply(s)) => {
                    if s.starts_with("\x1b_Gi=31;") && s.contains(";OK\x1b\\") {
                        kitty = true;
                    }
                    if let Some(body) = s.strip_prefix("\x1b[6;").and_then(|s| s.strip_suffix('t'))
                    {
                        let parts: Vec<_> = body
                            .split(';')
                            .filter_map(|s| s.parse::<u16>().ok())
                            .collect();
                        if parts.len() == 2
                            && parts[0] > 0
                            && parts[1] > 0
                            && parts[0] < 256
                            && parts[1] < 256
                        {
                            cap.cell = (parts[1], parts[0]);
                            cell = true;
                        }
                    }
                }
                Some(InputEvent::Key(k)) => input.pending.push_back(k),
                _ => {}
            }
        }
        cap.image = kitty && cell;
        cap.reason = if cap.image {
            "Kitty graphics acknowledged; cell pixel size confirmed".into()
        } else {
            format!("350 ms probe: Kitty ACK={kitty}, cell size={cell}; text fallback")
        };
        Ok(cap)
    }
    pub fn report(&self) -> String {
        format!(
            "tpreview {}\nstdout TTY: {}\nTERM: {}\ntmux: {}\nbackend: {}\nprobe: {}\ncell pixels: {} × {}{}\nMermaid: mmdflux 2.6.1 (flowchart, sequence); resvg 0.45\nfonts: bundled Noto Sans; local system fallback; --font adds TTF/OTF\nterminal font: cannot be queried; CJK/emoji width depends on terminal settings\nnetwork resources: disabled\n",
            env!("CARGO_PKG_VERSION"),
            io::stdout().is_terminal(),
            safe(&std::env::var("TERM").unwrap_or_default()),
            self.tmux,
            if self.image { "image / Kitty" } else { "text" },
            self.reason,
            self.cell.0,
            self.cell.1,
            if self.image {
                ""
            } else {
                " (fallback estimate, unused)"
            }
        )
    }
}
pub struct Guard {
    alternate: bool,
    pub stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    signals: Vec<signal_hook::SigId>,
}
impl Guard {
    pub fn enter(alternate: bool) -> io::Result<Self> {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut guard = Self {
            alternate: false,
            stop,
            signals: Vec::new(),
        };
        for signal in [
            signal_hook::consts::SIGINT,
            signal_hook::consts::SIGTERM,
            signal_hook::consts::SIGHUP,
        ] {
            guard
                .signals
                .push(signal_hook::flag::register(signal, guard.stop.clone())?);
        }
        terminal::enable_raw_mode()?;
        guard.alternate = alternate;
        if alternate {
            execute!(io::stdout(), EnterAlternateScreen, cursor::Hide)?;
        }
        Ok(guard)
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        if self.alternate {
            let _ = execute!(io::stdout(), cursor::Show, LeaveAlternateScreen);
        }
        for id in self.signals.drain(..) {
            signal_hook::low_level::unregister(id);
        }
    }
}
