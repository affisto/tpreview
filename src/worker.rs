use crate::{graphics, mermaid};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    io::{Read, Seek},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Diagram {
    pub text: String,
    pub png: Option<String>,
    pub width: u32,
    pub height: u32,
    pub image_error: Option<String>,
}
#[derive(Serialize, Deserialize)]
struct Request {
    source: String,
    image: bool,
    font: Option<PathBuf>,
}
#[derive(Serialize, Deserialize)]
struct Response {
    diagram: Option<Diagram>,
    error: Option<String>,
}

pub fn worker_main() -> Result<()> {
    // Hard ceilings in the disposable renderer; no limits are imposed on the interactive parent.
    unsafe {
        let cpu = libc::rlimit {
            rlim_cur: 4,
            rlim_max: 4,
        };
        libc::setrlimit(libc::RLIMIT_CPU, &cpu);
        let output = libc::rlimit {
            rlim_cur: 32 * 1024 * 1024,
            rlim_max: 32 * 1024 * 1024,
        };
        libc::setrlimit(libc::RLIMIT_FSIZE, &output);
        #[cfg(target_os = "linux")]
        {
            let memory = libc::rlimit {
                rlim_cur: 768 * 1024 * 1024,
                rlim_max: 768 * 1024 * 1024,
            };
            libc::setrlimit(libc::RLIMIT_AS, &memory);
        }
    }
    let request: Request = serde_json::from_reader(std::io::stdin().take(64 * 1024))?;
    let result = (|| -> Result<Diagram> {
        let rendered = mermaid::render(&request.source)?;
        let mut d = Diagram {
            text: rendered.text,
            png: None,
            width: 0,
            height: 0,
            image_error: None,
        };
        if request.image {
            match graphics::raster(&rendered.svg, request.font.as_deref()) {
                Ok((png, w, h)) => {
                    d.png = Some(png);
                    d.width = w;
                    d.height = h;
                }
                Err(e) => d.image_error = Some(crate::source::safe(&e.to_string())),
            }
        }
        Ok(d)
    })();
    let response = match result {
        Ok(d) => Response {
            diagram: Some(d),
            error: None,
        },
        Err(e) => Response {
            diagram: None,
            error: Some(format!("{e:#}")),
        },
    };
    serde_json::to_writer(std::io::stdout(), &response)?;
    Ok(())
}
pub fn isolated(
    source: &str,
    image: bool,
    font: Option<PathBuf>,
    generation: Option<(&AtomicU64, u64)>,
) -> Result<Diagram> {
    if source.len() > 16 * 1024 {
        bail!("diagram exceeds 16 KiB");
    }
    let mut input = tempfile::tempfile()?;
    serde_json::to_writer(
        &mut input,
        &Request {
            source: source.into(),
            image,
            font,
        },
    )?;
    input.rewind()?;
    let mut output = tempfile::tempfile()?;
    let mut child = Command::new(std::env::current_exe()?)
        .arg("--internal-render-worker")
        .stdin(input)
        .stdout(output.try_clone()?)
        .stderr(Stdio::null())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait()? {
            if !status.success() {
                bail!("renderer exited ({status}); resource limit or engine failure");
            }
            break;
        }
        if generation.is_some_and(|(g, v)| g.load(Ordering::Acquire) != v)
            || Instant::now() > deadline
        {
            let _ = child.kill();
            let _ = child.wait();
            bail!("render cancelled or exceeded 5 second limit");
        }
        thread::sleep(Duration::from_millis(10));
    }
    output.rewind()?;
    let response: Response = serde_json::from_reader(output.take(32 * 1024 * 1024))
        .context("invalid renderer response")?;
    response.diagram.ok_or_else(|| {
        anyhow::anyhow!(
            response
                .error
                .unwrap_or_else(|| "empty renderer response".into())
        )
    })
}
#[derive(Debug)]
pub struct Job {
    pub version: u64,
    pub diagrams: Vec<(usize, String, String)>,
}
#[derive(Debug)]
pub struct Completion {
    pub version: u64,
    pub block: usize,
    pub anchor: String,
    pub source: String,
    pub result: Result<Arc<Diagram>, String>,
    pub cached: bool,
}
pub struct Worker {
    tx: Option<mpsc::SyncSender<Job>>,
    pub rx: mpsc::Receiver<Completion>,
    pub version: Arc<AtomicU64>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Worker {
    pub fn new(image: bool, font: Option<PathBuf>) -> Self {
        let (tx, jobs) = mpsc::sync_channel::<Job>(1);
        let (results, rx) = mpsc::channel();
        let version = Arc::new(AtomicU64::new(0));
        let generation = version.clone();
        let handle = thread::spawn(move || {
            let mut cache = HashMap::<String, Arc<Diagram>>::new();
            let mut order = VecDeque::new();
            while let Ok(mut job) = jobs.recv() {
                while let Ok(new) = jobs.try_recv() {
                    job = new;
                }
                for (block, anchor, source) in job.diagrams {
                    if generation.load(Ordering::Acquire) != job.version {
                        break;
                    }
                    let cached = cache.contains_key(&source);
                    let result = if let Some(value) = cache.get(&source) {
                        Ok(value.clone())
                    } else {
                        isolated(
                            &source,
                            image,
                            font.clone(),
                            Some((&generation, job.version)),
                        )
                        .map(Arc::new)
                        .map_err(|e| e.to_string())
                    };
                    if let Ok(ref d) = result
                        && !cached
                    {
                        cache.insert(source.clone(), d.clone());
                        order.push_back(source.clone());
                    }
                    // Bound by both entry count and approximate stored payload bytes.
                    while cache.len() > 24
                        || cache
                            .values()
                            .map(|d| d.text.len() + d.png.as_ref().map_or(0, String::len))
                            .sum::<usize>()
                            > 32 * 1024 * 1024
                    {
                        if let Some(old) = order.pop_front() {
                            cache.remove(&old);
                        } else {
                            break;
                        }
                    }
                    if results
                        .send(Completion {
                            version: job.version,
                            block,
                            anchor,
                            source,
                            result,
                            cached,
                        })
                        .is_err()
                    {
                        return;
                    }
                }
            }
        });
        Self {
            tx: Some(tx),
            rx,
            version,
            thread: Some(handle),
        }
    }
    pub fn submit(&self, job: Job) -> Result<(), Job> {
        self.version.store(job.version, Ordering::Release);
        self.tx
            .as_ref()
            .unwrap()
            .try_send(job)
            .map_err(|e| match e {
                mpsc::TrySendError::Full(j) | mpsc::TrySendError::Disconnected(j) => j,
            })
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.version.store(u64::MAX, Ordering::Release);
        self.tx.take();
        if let Some(h) = self.thread.take() {
            let _ = h.join();
        }
    }
}
