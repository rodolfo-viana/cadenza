import { N_ } from './i18n.js';
import { KIND_LABEL } from './element-label.js';
import { normalizeParagraphs } from './pptx-ooxml.js';
// PPTX is a conversion boundary: native objects where the properties map,
// otherwise a PNG from the same native renderer used by Cadenza. Nothing in
// this module reads the active document or writes files.
import { measureSnapshot, textRuns, color, px, fontFace, checkAbort, bounded } from './pptx-layout.js';

let library;
function loadLibrary(signal) {
  if (window.PptxGenJS) return Promise.resolve(window.PptxGenJS);
  if (!library) library = new Promise((resolve, reject) => {
    const script = document.createElement('script');
    script.src = new URL('../vendor/pptxgenjs/pptxgen.bundle.js', import.meta.url).href;
    script.onload = () => window.PptxGenJS ? resolve(window.PptxGenJS) : reject(new Error(N_('PowerPoint exporter did not load.')));
    script.onerror = () => reject(new Error(N_('PowerPoint exporter could not be loaded.')));
    document.head.appendChild(script);
  }).catch(error => { library = null; throw error; });
  return bounded(library, signal,15000,N_('The PowerPoint export library did not finish loading.'));
}
const inch = v => Number(v) / 96;
const scaleOf = e => Number(e.contentScale) > 0 ? Number(e.contentScale) : 1;
const opacityOf = e => Math.max(0,Math.min(1,Number(e.opacity ?? 1)));
const rotationOf = e => ((Number(e.rotation || 0)%360)+360)%360;
const effect = e => [e.style?.shadow,e.content?.style?.shadow,e.content?.text?.style?.shadow].some(shadow=>shadow && shadow!=='none');
const decorated = e => effect(e) || Number(e.style?.radius)>0 || Number(e.style?.borderWidth)>0 || Number(e.style?.padding)>0 ||
  (!!e.style?.fill && color(e.style.fill).transparency<100);
const solid = paint => ({ color: paint.color, transparency: Math.max(0,Math.min(100,paint.transparency)) });
function fade(paint, opacity) { return { ...paint, transparency: 100-(100-paint.transparency)*opacity }; }
function fadeText(parsed, opacity, hyperlink) {
  return parsed.runs.map(run=>({ ...run, options:{...run.options,
    hyperlink:run.options.hyperlink || (hyperlink ? {...hyperlink} : undefined),
    transparency:100-(100-(run.options.transparency||0))*opacity} }));
}
function geometry(e, node) {
  const h = e.content?.kind === 'text' && e.content.style?.autoHeight && node?.offsetHeight ? node.offsetHeight : e.h;
  return { x: inch(e.x), y: inch(e.y), w: inch(e.w), h: inch(h), rotate: rotationOf(e), objectName: e.name || e.id };
}
function actionLink(action, slides, current) {
  if (action?.kind === 'url') {
    const url = String(action.url || '').trim();
    if (/^(https?:|mailto:)/i.test(url)) return { url };
    if (url && !url.includes(':')) return { url: `https://${url}` };
  }
  if (action?.kind === 'slide' && slides.has(action.slide)) return { slide: slides.get(action.slide) };
  if (action?.kind === 'next' && current < slides.size) return { slide: current+1 };
  return undefined;
}
function findNode(root, e) {
  return [...root.querySelectorAll('.cz-el')].find(n => n.getAttribute('data-cz-id') === e.id || n.getAttribute('data-id') === (e.link || e.id));
}

/** Geometry for an editable picture with PowerPoint's native crop operation. */
export function pictureGeometry(e) {
  const image = e.content, nw = Number(image.naturalWidth), nh = Number(image.naturalHeight);
  if (!(nw>0 && nh>0 && e.w>0 && e.h>0)) return null;
  const c = image.crop || { x:0,y:0,w:1,h:1 };
  if (![c.x,c.y,c.w,c.h].every(Number.isFinite) || c.w<=0 || c.h<=0 || c.x<0 || c.y<0 || c.x+c.w>1.000001 || c.y+c.h>1.000001) return null;
  const iw = nw*c.w, ih = nh*c.h, ratio = image.fit === 'contain' ? Math.min(e.w/iw,e.h/ih) : Math.max(e.w/iw,e.h/ih);
  const rw = image.fit === 'fill' ? e.w : iw*ratio, rh = image.fit === 'fill' ? e.h : ih*ratio;
  const rx = (e.w-rw)*Math.max(0,Math.min(100,image.focusX ?? 50))/100;
  const ry = (e.h-rh)*Math.max(0,Math.min(100,image.focusY ?? 50))/100;
  const fullW=rw/c.w, fullH=rh/c.h;
  const left=Math.max(0,rx), top=Math.max(0,ry), w=Math.min(e.w,rx+rw)-left, h=Math.min(e.h,ry+rh)-top;
  if (!(w>0 && h>0)) return null;
  let cx=left+w/2, cy=top+h/2;
  if (image.flipX) cx=e.w-cx;
  if (image.flipY) cy=e.h-cy;
  const angle=rotationOf(e)*Math.PI/180, dx=cx-e.w/2, dy=cy-e.h/2;
  cx=e.x+e.w/2+dx*Math.cos(angle)-dy*Math.sin(angle);
  cy=e.y+e.h/2+dx*Math.sin(angle)+dy*Math.cos(angle);
  return { x:inch(cx-w/2), y:inch(cy-h/2), w:inch(fullW), h:inch(fullH), rotate:rotationOf(e), flipH:!!image.flipX, flipV:!!image.flipY,
    sizing:{type:'crop',x:inch(left-rx+c.x*fullW),y:inch(top-ry+c.y*fullH),w:inch(w),h:inch(h)} };
}

function nativeText(slide, e, node, read, slideNumbers, current) {
  const textNode = node?.querySelector('.cz-text:not(.cz-el)');
  const parsed = textRuns(textNode,read.style,scaleOf(e),slideNumbers);
  if (!parsed || effect(e) || Number(e.style?.radius)>0) return false;
  const cs = read.style(textNode), box = node.querySelector('.cz-text-box,.cz-shape-text');
  const bs = read.style(box || node), g = geometry(e,node), scale=scaleOf(e);
  const fill = fade(color(bs.backgroundColor),opacityOf(e));
  const border = fade(color(bs.borderColor),opacityOf(e));
  const options = { ...g, ...parsed, color:parsed.color.color, margin:[px(bs.paddingTop),px(bs.paddingRight),px(bs.paddingBottom),px(bs.paddingLeft)].map(v=>v*scale*.75),
    fontSize:parsed.fontSize, breakLine:false, fit:'none', wrap:true,
    valign:e.content.style?.valign || e.content.text?.style?.valign || 'top',
    transparency:100-(100-parsed.color.transparency)*opacityOf(e)*Number(cs.opacity||1),
    line: px(bs.borderWidth)>0 ? {color:border.color,transparency:border.transparency,width:px(bs.borderWidth)*scale*.75} : {transparency:100},
    fill:solid(fill), paraSpaceAfter:0 };
  delete options.runs;
  slide.addText(fadeText(parsed,opacityOf(e)*Number(cs.opacity||1),actionLink(e.action,slideNumbers,current)),options); return true;
}

const SHAPES = { rect:'rect',ellipse:'ellipse',triangle:'triangle',diamond:'diamond',pentagon:'pentagon',hexagon:'hexagon',octagon:'octagon',line:'line' };
function nativeShape(slide,e,node,read,slideNumbers,current) {
  const type=SHAPES[e.content.shape];
  if (!type || effect(e) || Number(e.style?.radius)>0) return false;
  const textNode=node?.querySelector('.cz-text:not(.cz-el)');
  const parsed=textNode ? textRuns(textNode,read.style,scaleOf(e),slideNumbers) : null;
  if (textNode?.textContent.trim() && !parsed) return false;
  const path=node?.querySelector('.cz-shape-svg path,.cz-shape-svg line'); if(!path)return false;
  const cs=read.style(path), scale=scaleOf(e), stroke=(Number(e.style?.borderWidth)|| (type==='line'?1:0))*scale;
  const line=fade(color(cs.stroke),opacityOf(e)), fill=fade(color(cs.fill),opacityOf(e));
  const g=geometry(e,node), inset=type==='line'?0:stroke/192;
  slide.addShape(type,{...g,x:g.x+inset,y:g.y+inset,w:Math.max(.001,g.w-inset*2),h:Math.max(.001,g.h-inset*2),
    fill:type==='line'?{transparency:100}:solid(fill),line:{...line,width:stroke*.75,
      dashType:e.style?.borderStyle==='dashed'?'dash':e.style?.borderStyle==='dotted'?'sysDot':'solid',
      beginArrowType:e.content.startCap==='dot'?'oval':e.content.startCap==='arrow'?'triangle':'none',
      endArrowType:e.content.endCap==='dot'?'oval':e.content.endCap==='arrow'?'triangle':'none'},
    hyperlink:actionLink(e.action,slideNumbers,current)});
  if (parsed && textNode.textContent.trim()) {
    const box=node.querySelector('.cz-shape-text'), bs=read.style(box);
    const options={...g,...parsed,color:parsed.color.color,margin:[px(bs.paddingTop),px(bs.paddingRight),px(bs.paddingBottom),px(bs.paddingLeft)].map(v=>v*scale*.75),
      valign:e.content.text?.style?.valign||'middle',fit:'none',wrap:true,transparency:100-opacityOf(e)*100,
      fill:{transparency:100},line:{transparency:100}};
    delete options.runs; slide.addText(fadeText(parsed,opacityOf(e)*Number(read.style(textNode).opacity||1),actionLink(e.action,slideNumbers,current)),options);
  }
  return true;
}

function nativeTable(slide,e,node,read,slideNumbers) {
  if (rotationOf(e) || opacityOf(e)!==1 || decorated(e)) return false;
  const table=node?.querySelector('table.cz-table'); if(!table || !table.rows.length)return false;
  const scale=scaleOf(e), rows=[];
  for(const row of table.rows){
    const cells=[];
    for(const cell of row.cells){
      const parsed=textRuns(cell,read.style,scale,slideNumbers); if(!parsed)return false;
      const cs=read.style(cell), border=color(cs.borderBottomColor);
      cells.push({text:parsed.runs,options:{fontFace:parsed.fontFace,fontSize:parsed.fontSize,color:parsed.color.color,
        fill:solid(color(cs.backgroundColor)),bold:Number(cs.fontWeight)>=600,align:cs.textAlign,valign:'middle',
        margin:[px(cs.paddingTop),px(cs.paddingRight),px(cs.paddingBottom),px(cs.paddingLeft)].map(v=>v*scale*.75),
        border:[{type:'none'},{type:'none'},px(cs.borderBottomWidth)>0?{type:'solid',pt:px(cs.borderBottomWidth)*scale*.75,color:border.color}:{type:'none'},{type:'none'}]}});
    }
    rows.push(cells);
  }
  const g=geometry(e,node);
  slide.addTable(rows,{...g,autoPage:false,margin:0,border:{type:'none'},
    colW:[...table.rows[0].cells].map(c=>inch(c.offsetWidth*scale)),rowH:[...table.rows].map(r=>inch(r.offsetHeight*scale)),
    fontFace:fontFace(read.style(table).fontFamily),fontSize:e.content.fontSize*scale*.75,verbose:false});
  return true;
}

const CHARTS={column:'bar',bar:'bar',line:'line',area:'area',pie:'pie',donut:'doughnut','stacked-column':'bar','stacked-bar':'bar','stacked-area':'area'};
function nativeChart(slide,e,node,read) {
  const c=e.content,type=CHARTS[c.chart];
  if(!type||rotationOf(e)||opacityOf(e)!==1||decorated(e)||c.errorBars||c.refLines?.length||c.links?.length||!c.series?.length||!c.labels?.length)return false;
  if(['pie','donut'].includes(c.chart)&&c.series.length!==1)return false;
  if(c.series.some(s=>s.marker && s.marker!=='circle'))return false;
  const cs=read.style(node.querySelector('.cz-chart-box')||node), scale=scaleOf(e), fg=color(cs.color).color;
  const round=['pie','donut'].includes(c.chart);
  let palette=c.series.map((s,i)=>color(s.color||cs.getPropertyValue(`--cz-chart-${i%6+1}`),'6366F1').color);
  if(round){
    // Read the actual renderer's category fills, including its lighter/darker
    // colors after category six. Never substitute a single series color.
    if(!c.labels.every((_,i)=>Number.isFinite(c.series[0].values?.[i])&&c.series[0].values[i]>0))return false;
    const slices=[...node.querySelectorAll('.cz-chart-svg path[style],.cz-chart-svg circle[style]')];
    if(slices.length!==c.labels.length)return false;
    palette=slices.map(slice=>color(read.style(slice).fill).color);
  }
  // PptxGenJS 4.0.1 uses `value || ''` in its category-chart workbook writer.
  // The truthy numeric lexical form preserves zero as an OOXML numeric cell
  // without conflating it with a blank or modifying the pinned vendor bundle.
  const data=c.series.map(s=>({name:s.name||'',labels:[...c.labels],values:c.labels.map((_,i)=>s.values?.[i]===0?'0':Number.isFinite(s.values?.[i])?s.values[i]:null)}));
  const options={...geometry(e,node),showLegend:!!c.showLegend,showValue:!!c.showValues,legendPos:'b',legendColor:fg,
    fontFace:fontFace(cs.fontFamily),legendFontFace:fontFace(cs.fontFamily),legendFontSize:c.fontSize*scale*.75,
    catAxisLabelColor:fg,valAxisLabelColor:fg,catAxisLabelFontSize:c.fontSize*scale*.75,valAxisLabelFontSize:c.fontSize*scale*.75,
    catAxisTitleColor:fg,valAxisTitleColor:fg,dataLabelColor:fg,dataLabelFontSize:c.fontSize*scale*.75,
    showCatAxisTitle:!!c.xTitle,catAxisTitle:c.xTitle||'',showValAxisTitle:!!c.yTitle,valAxisTitle:c.yTitle||'',
    chartColors:palette,showBorder:false,showCatName:false,showTitle:false,
    chartArea:{fill:{color:'FFFFFF',transparency:100}},
    plotArea:{fill:{color:'FFFFFF',transparency:100}},
    valGridLine:c.showGrid?{color:'94A3B8',size:.5}:{style:'none'},catGridLine:{style:'none'},
    displayBlanksAs:'gap',showMarker:c.chart==='line',lineSize:2,barDir:c.chart.endsWith('bar')?'bar':'col',
    barGrouping:c.stacked||c.chart.startsWith('stacked-')?'stacked':'clustered',
    showPercent:round&&!!c.showValues,showLabel:false};
  if(round){options.showValue=false;options.holeSize=58;options.firstSliceAng=0;}
  if(c.axisMin!==null&&Number.isFinite(c.axisMin))options.valAxisMinVal=c.axisMin;
  if(c.axisMax!==null&&Number.isFinite(c.axisMax))options.valAxisMaxVal=c.axisMax;
  slide.addChart(type,data,options);return true;
}

/**
 * @param {object} snapshot Immutable deck, rendered slides and pinned job assets.
 * @param {object} dependencies capture(slideId,{elementId}|{background:true}|{}), asset(name), AbortSignal and progress callback.
 * @returns {Promise<{bytes:ArrayBuffer,issues:object[]}>}
 */
export async function buildPptx(snapshot,{capture,asset,signal,onProgress}={}) {
  checkAbort(signal);
  if(!snapshot?.deck||!Array.isArray(snapshot.slides)||!(snapshot.width>0&&snapshot.height>0))throw new Error(N_('Invalid presentation snapshot.'));
  if(typeof capture!=='function'||typeof asset!=='function')throw new Error(N_('PowerPoint export needs the native capture and pinned asset readers.'));
  const deck=snapshot.deck, slides=deck.slides.filter(s=>!s.hidden);
  if(!slides.length)throw new Error(N_('All slides are skipped — there is nothing to export.'));
  const PptxGenJS=await loadLibrary(signal);checkAbort(signal);
  const pptx=new PptxGenJS();pptx.defineLayout({name:'CADENZA',width:inch(snapshot.width),height:inch(snapshot.height)});pptx.layout='CADENZA';
  pptx.title=deck.title||'Presentation';pptx.author=deck.author||'';pptx.subject='Exported from Cadenza';pptx.company='';pptx.lang='en-US';
  const issues=[],numbers=new Map(slides.map((s,i)=>[s.id,i+1]));
  const issue=(code,slide,element,feature,disposition,message)=>issues.push({code,severity:'warning',slide,element:element||undefined,feature:KIND_LABEL[feature]||feature,disposition,message});
  const rendered=new Map(snapshot.slides.map(s=>[s.id,s]));
  if(slides.some(s=>s.elements.some(e=>e.group)))issue('PPTX_GROUPS_FLATTENED',0,null,N_('Groups'),'omitted',N_('Grouped elements keep their placement, but editing groups are not transferred to PowerPoint.'));
  if(deck.webFonts?.length)issue('PPTX_FONTS_NOT_EMBEDDED',0,null,N_('fonts'),'approximated',N_('PowerPoint needs the original fonts installed; web fonts are not embedded in this export.'));
  if(deck.settings?.transition!=='none'||slides.some(s=>s.autoAnimate||s.autoSlide||s.elements.some(e=>e.build?.effect||e.build?.outEffect||e.build?.path?.length)))
    issue('PPTX_STATIC_PRESENTATION',0,null,N_('transitions and builds'),'omitted',N_('The export contains static slide content; Cadenza transitions, builds and timing are not transferred.'));
  let measurement;
  const addRaster=async(slide,s,index,e,reason)=>{
    checkAbort(signal);
    const image=await bounded(capture(s.id,e?{elementId:e.id}:{}),signal,45000);checkAbort(signal);
    if(!image?.data?.startsWith('data:image/png')||![image.x,image.y,image.w,image.h].every(Number.isFinite)||image.w<=0||image.h<=0)throw new Error(N_('Native capture returned an invalid image.'));
    slide.addImage({data:image.data,x:inch(image.x),y:inch(image.y),w:inch(image.w),h:inch(image.h),objectName:e?.name||e?.id||s.id,
      altText:e?.content?.alt||e?.name||'Cadenza content',hyperlink:e?actionLink(e.action,numbers,index):undefined});
    issue('PPTX_RASTERIZED',index,e?.id,e?.content?.kind||N_('Slide'),'rasterized',reason);
  };
  try{
    if(!deck.customCss?.trim())measurement=await measureSnapshot(snapshot,signal);
    for(let i=0;i<slides.length;i++){
      checkAbort(signal);const source=slides[i],slide=pptx.addSlide();
      onProgress?.({phase:'mapping',completed:i,total:slides.length});
      if(source.notes)slide.addNotes(source.notes);
      if(deck.customCss?.trim()){
        await addRaster(slide,source,i+1,null,N_('Custom CSS is preserved as a full-slide image; slide content is not editable in PowerPoint.'));continue;
      }
      const item=rendered.get(source.id);if(!item)throw new Error(N_('A rendered slide is missing from the export snapshot.'));
      const read=await measurement.slide(item);
      const background=await bounded(capture(source.id,{background:true}),signal,45000);checkAbort(signal);
      if(!background?.data?.startsWith('data:image/png'))throw new Error(N_('Native capture returned an invalid background.'));
      slide.addImage({data:background.data,x:0,y:0,w:inch(snapshot.width),h:inch(snapshot.height),objectName:'Cadenza background'});
      for(const e of source.elements){
        checkAbort(signal);const kind=e.content?.kind,node=findNode(read.root,e);let mapped=false;
        if(kind==='camera'){issue('PPTX_CAMERA_OMITTED',i+1,e.id,'camera','omitted',N_('Live camera content is not included in the exported file.'));continue;}
        if(!node)throw new Error(N_('An element is missing from the rendered export snapshot.'));
        if(kind==='diagram'){
          const c=e.content, cached=c.renderedFor;
          if(!cached||cached.source!==(c.source||'')||cached.style!==(c.style||'classic')||cached.look!==(c.look||'classic'))
            issue('PPTX_DIAGRAM_STALE',i+1,e.id,'diagram','approximated',N_('The diagram preview does not represent its current source. The last available preview is exported; correct the Mermaid source and export again.'));
        }
        if(kind==='text')mapped=nativeText(slide,e,node,read,numbers,i+1);
        else if(kind==='shape')mapped=nativeShape(slide,e,node,read,numbers,i+1);
        else if(kind==='table')mapped=nativeTable(slide,e,node,read,numbers);
        else if(kind==='chart')mapped=nativeChart(slide,e,node,read);
        else if(kind==='image'&&!decorated(e)&&![e.content.grayscale,e.content.blur].some(v=>v>0)&&['brightness','contrast','saturate'].every(k=>e.content[k]===undefined||e.content[k]===100)){
          const picture=pictureGeometry(e);
          if(picture && !/^https?:/i.test(e.content.src)){
            const data=e.content.src.startsWith('data:')?e.content.src:await bounded(asset(e.content.src),signal);checkAbort(signal);
            if(/^data:image\/(png|jpeg);base64,/i.test(data||'')){
              slide.addImage({data,...picture,transparency:100-opacityOf(e)*100,altText:e.content.alt||'',objectName:e.name||e.id,hyperlink:actionLink(e.action,numbers,i+1)});mapped=true;
            }
          }
        }
        if(!mapped)await addRaster(slide,source,i+1,e,N_('This appearance is preserved as a static image because its properties do not map faithfully to native PowerPoint objects.'));
        else if(['text','table','chart'].includes(kind))issue('PPTX_NATIVE_LAYOUT',i+1,e.id,kind,'approximated',N_('The content remains editable; PowerPoint may lay out text, table cells or chart axes differently.'));
        if(e.dataSource)issue('PPTX_SOURCE_NOT_EMBEDDED',i+1,e.id,N_('source data'),'omitted',N_('The original source text is kept in the Cadenza file; this export carries the displayed data only.'));
      }
    }
    checkAbort(signal);onProgress?.({phase:'writing',completed:slides.length,total:slides.length});
    const raw=await bounded(pptx.write({outputType:'arraybuffer',compression:true}),signal,120000);checkAbort(signal);
    const bytes=await bounded(normalizeParagraphs(raw,signal),signal,120000);checkAbort(signal);
    return {bytes,issues};
  }finally{measurement?.dispose();}
}
