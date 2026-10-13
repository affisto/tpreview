import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { resolve, extname } from 'node:path';
import { editorHtml } from '../src/html';
const fixture = '# Working notes\n\nA **visual** Markdown editor.\n\n- [ ] Review draft\n- [x] Read the brief\n\n| Area | Status |\n| --- | --- |\n| Editor | Ready |\n\n```mermaid\nflowchart LR\n    Draft --> Review\n    Review --> Publish\n```\n';
createServer(async (req, res) => {
  if (req.url === '/') {
    const boot = `<script nonce="test">window.fixture=${JSON.stringify(fixture)}; window.__host={text:window.fixture,version:1,edits:[],messages:[],delay:0,history:[],future:[],state:{},saved:''};
window.acquireVsCodeApi=()=>({getState:()=>window.__host.state,setState:s=>window.__host.state=s,postMessage:m=>{
 const h=window.__host;h.messages.push(m);
 if(m.type==='ready')setTimeout(()=>window.externalChange(h.text,false),0);
 if(m.type==='edit')setTimeout(()=>{const accepted=m.version===h.version;if(accepted){h.history.push(h.text);h.future=[];h.text=m.text;h.version++;h.edits.push(m)}window.postMessage({type:'ack',id:m.id,accepted,text:h.text,version:h.version},'*')},h.delay);
 if(m.type==='save')h.saved=h.text;
 if(m.type==='undo'&&h.history.length){h.future.push(h.text);window.externalChange(h.history.pop())}
 if(m.type==='redo'&&h.future.length){h.history.push(h.text);window.externalChange(h.future.pop())}
}});
window.externalChange=(text,increment=true)=>{const h=window.__host;h.text=text;if(increment)h.version++;window.postMessage({type:'document',text:h.text,version:h.version,name:'notes.md',baseUri:location.origin+'/'},'*')};
</script>`;
    res.setHeader('Content-Type', 'text/html');
    res.end(editorHtml('/main.js', '/main.css', "'self'", 'test').replace('</head>', boot + '</head>')); return;
  }
  try {
    const path = resolve('dist/webview', '.' + new URL(req.url!, 'http://localhost').pathname);
    if (!path.startsWith(resolve('dist/webview') + '/')) throw new Error();
    res.setHeader('Content-Type', extname(path) === '.css' ? 'text/css' : 'text/javascript'); res.end(await readFile(path));
  } catch { res.statusCode = 404; res.end(); }
}).listen(4178, '127.0.0.1');
