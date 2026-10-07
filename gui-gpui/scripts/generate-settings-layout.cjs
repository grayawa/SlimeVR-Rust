/** Extract original settings structure and control metadata, without executing React. */
const fs = require('fs'); const path = require('path'); const { createRequire } = require('module');
const root = path.resolve(__dirname, '../..'); const ts = createRequire(path.join(root,'gui/package.json'))('typescript');
const generated=fs.readFileSync(path.join(root,'gui-gpui/src/rpc_generated.rs'),'utf8');
const schema=JSON.parse(generated.match(/from_str\(r###"([\s\S]*?)"###/)[1]);
const norm=s=>s.replace(/[^a-z0-9]/gi,'').toLowerCase();
const roots={trackers:['steam_vr_trackers'], filtering:['filtering'],toggles:['model_settings','toggles'],ratios:['model_settings','ratios'],legTweaks:['model_settings','leg_tweaks'],tapDetection:['tap_detection_settings'],resetsSettings:['resets_settings'],hidSettings:['hid_settings'],velocitySettings:['velocity_settings'],stayAligned:['stay_aligned'],router:['osc_router'],vrchat:['vrc_osc'],vmc:['vmc_osc']};
function resolve(name){
 const bits=name.split('.');const prefix=bits.shift();
 if(['notifications','behavior','appearance'].includes(prefix))return {preference:bits.join('/')};
 if(prefix==='developer')return {preference:bits[0]==='enabled'?'debug':'devSettings/'+bits.join('/')};
 if(!roots[prefix])throw new Error('Unmapped control '+name);
 let typ='SettingsResponse',parts=[];
 for(const bit of [...roots[prefix],...bits]){const f=schema[typ]?.find(f=>norm(f.name)===norm(bit));if(!f)throw new Error('Missing protocol field '+name+' '+bit);parts.push(f.name);typ=f.type;}
 return {path:'/'+parts.join('/'),field_type:typ};
}
const out={};
for(const file of ['GeneralSettings.tsx','components/StayAlignedSettings.tsx','OSCRouterSettings.tsx','VRCOSCSettings.tsx','VMCSettings.tsx','InterfaceSettings.tsx','AdvancedSettings.tsx']){
 const full=path.join(root,'gui/src/components/settings/pages',file);const source=fs.readFileSync(full,'utf8');const ast=ts.createSourceFile(full,source,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX);
 const text=n=>n?.getText(ast)||'';
 const attrs=n=>Object.fromEntries(n.attributes.properties.filter(ts.isJsxAttribute).map(a=>[a.name.getText(ast),a.initializer?(ts.isStringLiteral(a.initializer)?a.initializer.text:ts.isJsxExpression(a.initializer)?text(a.initializer.expression):text(a.initializer)):true]));
 const literal=x=>typeof x==='string'?x.replace(/^['"]|['"]$/g,''):'';
 const label=x=>typeof x==='string'?(x.match(/getString\(\s*['"]([^'"]+)['"]/s)?.[1]||literal(x)):'';
 function convert(n, inheritedLabel=''){
  if(ts.isJsxText(n))return [];
  if(ts.isParenthesizedExpression(n))return convert(n.expression,inheritedLabel);
  if(ts.isJsxFragment(n))return n.children.flatMap(c=>convert(c,inheritedLabel));
  if(ts.isJsxExpression(n)){
   const e=n.expression;if(!e)return [];
   if(ts.isBinaryExpression(e)&&e.operatorToken.kind===ts.SyntaxKind.AmpersandAmpersandToken){const children=convert(e.right,inheritedLabel);return children.length?[{kind:'condition',guard:text(e.left),children}]:[];}
   const msg=text(e).match(/(?:l10n\s*\.\s*)?getString\(\s*['"]([^'"]+)['"]/s);
   if(msg&&(!/<[A-Z]/.test(text(e))||/\.split\(/.test(text(e))))return [{kind:'text',label:msg[1]}];
   return convert(e,inheritedLabel);
  }
  if(!ts.isJsxElement(n)&&!ts.isJsxSelfClosingElement(n)){
   if(ts.isBinaryExpression(n)&&n.operatorToken.kind===ts.SyntaxKind.AmpersandAmpersandToken){const children=convert(n.right,inheritedLabel);return children.length?[{kind:'condition',guard:text(n.left),children}]:[];}
   const children=[];ts.forEachChild(n,c=>children.push(...convert(c,inheritedLabel)));return children;
  }
  const opening=ts.isJsxElement(n)?n.openingElement:n;const tag=text(opening.tagName);const a=attrs(opening);const children=()=>ts.isJsxElement(n)?n.children.flatMap(c=>convert(c,inheritedLabel)):[];
  if(tag==='Localized'){
   const id=literal(a.id);const converted=ts.isJsxElement(n)?n.children.flatMap(c=>convert(c,id)):[];
   return converted.length?converted:[{kind:'text',label:id}];
  }
  if(tag==='Typography'){
   const id=a.id?literal(a.id):text(n).match(/getString\(\s*['"]([^'"]+)['"]/s)?.[1]||inheritedLabel;
   return id?[{kind:'text',label:id,variant:a.variant||'',color:a.color||'',bold:!!a.bold}]:[];
  }
  if(['CheckBox','NumberSelector','Radio','Dropdown','Input','SystemFileInput'].includes(tag)){
   if(!a.name)return [];
   if(a.name.includes('`'))return tag==='CheckBox'?[{kind:'action',widget:'DeveloperToggles'}]:[];
   const name=literal(a.name);const node={kind:'control',widget:tag,name,label:a.label?label(a.label):inheritedLabel,...resolve(name)};
   for(const key of ['min','max','step','value','description','disabled'])if(a[key]!==undefined)node[key]=key==='description'?label(a[key]):literal(String(a[key]));
   if(a.valueLabelFormat)node.unit=a.valueLabelFormat.includes('percentage')?'percent':a.valueLabelFormat.includes('seconds')?'seconds':'';
   return [node];
  }
  if(tag==='ThemeSelector')return literal(a.value)==='slime'?[{kind:'action',widget:'ThemeSelector'}]:[];
  if(tag==='LangSelector'||tag==='Range')return [{kind:'action',widget:tag}];
  if(['Button','MagnetometerToggleSetting','VMCFileUpload','CopySettingsButton'].includes(tag))return [{kind:'action',widget:tag,label:a.id?literal(a.id):label(text(n).match(/getString\(\s*['"]([^'"]+)['"]/s)?.[0]||'')||inheritedLabel,target:a.to||'',id:a.id||'',callback:a.onClick||'',disabled:a.disabled||''}];
  const kids=children();if(!kids.length)return [];
  if(tag==='div')return [{kind:'container',classes:a.className||'',children:kids}];
  return kids;
 }
 function visit(n){
  if(ts.isJsxElement(n)&&text(n.openingElement.tagName)==='SettingsPagePaneLayout'){
   const a=attrs(n.openingElement);const id=literal(a.id);if(id){const icon=(a.icon||'').match(/<([A-Za-z]+)Icon/)?.[1]||'Wrench';out[id]={icon,source:path.relative(root,full),children:n.children.flatMap(c=>convert(c))};}return;
  }
  ts.forEachChild(n,visit);
 }
 visit(ast);
}
const names=fs.readFileSync(path.join(root,'gui/src/i18n/names.ts'),'utf8');
out.__languages=[...names.matchAll(/name:\s*'([^']+)'[\s\S]*?key:\s*'([^']+)'/g)].map(m=>({key:m[2],name:m[1]}));
fs.writeFileSync(path.join(root,'gui-gpui/src/settings_layout.json'),JSON.stringify(out,null,2)+'\n');
console.log('Extracted original panes: '+Object.keys(out).join(', '));
