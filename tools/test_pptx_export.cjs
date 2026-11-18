// Browser mapper + structural OOXML regression. Uses a fresh headless profile.
// cargo run --manifest-path src-tauri/Cargo.toml --example pptx_fixture > /tmp/pptx-snapshot.json
// node tools/test_pptx_export.cjs /tmp/pptx-snapshot.json [artifact-directory] [chromium-binary]
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),os=require('node:os');
const http=require('node:http'),cp=require('node:child_process');
const {withBrowser}=require('./browser_harness.cjs');
const root=path.resolve(__dirname,'..'),snapshot=fs.readFileSync(process.argv[2]);
const output=process.argv[3]||fs.mkdtempSync(path.join(os.tmpdir(),'cadenza-pptx-'));
const browser=process.argv[4]||process.env.CHROMIUM||cp.spawnSync('which',['chromium'],{encoding:'utf8'}).stdout.trim();
assert(browser,'Chromium is required');fs.mkdirSync(output,{recursive:true});
const mime={'.html':'text/html','.js':'text/javascript','.css':'text/css','.json':'application/json','.woff2':'font/woff2','.png':'image/png'};
const server=http.createServer((request,response)=>{
  try {const name=decodeURIComponent(new URL(request.url,'http://localhost').pathname),file=path.resolve(root,`.${name}`);assert(file.startsWith(root+path.sep));
    const bytes=name==='/snapshot.json'?snapshot:fs.readFileSync(file);
    response.writeHead(200,{'Content-Type':mime[path.extname(file)]||'application/octet-stream'});response.end(bytes);
  }catch{response.writeHead(404);response.end();}
});
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function until(check){for(let i=0;i<100;i++){const result=await check();if(result)return result;await delay(100);}throw new Error('Browser timed out');}
(async()=>{
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  const cache=browser.startsWith('/snap/')?path.join(os.homedir(),'snap/chromium/common'):os.tmpdir();fs.mkdirSync(cache,{recursive:true});
  const profile=fs.mkdtempSync(path.join(cache,'cadenza-pptx-test-'));
  const checkExport=async({call,evaluate:evaluateExpression})=>{
    const evaluate=expression=>evaluateExpression(expression,{awaitPromise:true});
    await call('Page.navigate',{url:`http://127.0.0.1:${server.address().port}/tools/test_pptx_export.html`});
    await until(()=>evaluate("typeof window.run==='function'"));
    const result=await evaluate('window.run()');
    fs.writeFileSync(path.join(output,'export.pptx'),Buffer.from(result.bytes,'base64'));delete result.bytes;
    fs.writeFileSync(path.join(output,'results.json'),JSON.stringify(result,null,2));
    console.log(`PASS: ${result.assertions} browser/OOXML assertions. Artifact: ${path.join(output,'export.pptx')}`);
  };
  try {
    await withBrowser([browser],profile,checkExport,{commandMs:60000,logFile:path.join(output,'browser.log'),ownProcessGroup:true});
  }finally{server.close();await delay(200);fs.rmSync(profile,{recursive:true,force:true});}
})().catch(error=>{console.error(error);server.close();process.exitCode=1;});
