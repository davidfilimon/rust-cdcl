'use strict';
const fs=require('fs'),vm=require('vm'),assert=require('assert');
class Element{
 constructor(tag='div'){this.tagName=tag.toUpperCase();this.children=[];this.value='';this.textContent='';this.disabled=false;this.hidden=false;this.className='';this.style={};this.classList={toggle(){},add(){},remove(){},contains(){return false}};}
 append(...nodes){this.children.push(...nodes)}replaceChildren(...nodes){this.children=nodes;this.textContent=''}
 addEventListener(k,fn){this['on'+k]=fn}setAttribute(k,v){this[k]=v}
}
const store=new Map(),document={getElementById(k){if(!store.has(k))store.set(k,new Element());return store.get(k)},createElement(t){return new Element(t)},querySelectorAll(k){return k==='.example'?store.get('examples').children:[]},addEventListener(){},body:new Element(),activeElement:null};
const $=k=>document.getElementById(k);
const defs=[{name:'a',expression:'x+1 > 0'},{name:'b',expression:'x+y > 0'},{name:'c',expression:'x < 0'},{name:'d',expression:'x+y > 4'}];
const initial={status:'OK',source:'none',message:'Context consistent',trail:[-2,1],levels:[0,0],clause:[],lemma:[],propagated:[{literal:-2,clause:[-2]},{literal:1,clause:[1,2]}],witness:{}};
const afterLemma={status:'UNSAT',source:'theory',message:'Conflict !b ∧ d',trail:[-2,1,-3,4],levels:[0,0,0,0],clause:[2,-4],lemma:[2,-4],propagated:[{literal:-2,clause:[-2]},{literal:1,clause:[1,2]},{literal:-3,clause:[-1,-3]},{literal:4,clause:[3,4]}],witness:{}};
const afterBoth={status:'UNSAT',source:'boolean',message:'Conflict Boolean at root',trail:[-2,1,-3],levels:[0,0,0],clause:[3,4],lemma:[],propagated:[],witness:{}};
let requests=[];
async function fetch(url,opt){
 requests.push({url,headers:opt.headers});
 assert.equal(url,'/api/manual');
 const learnt=opt.headers['X-Manual-Lemmas'].includes('-1,-3');
 const both=opt.headers['X-Manual-Lemmas'].includes('2,-4');
 const candidate=Number(opt.headers['X-Manual-Candidate']||0);
 const base=both?afterBoth:learnt?afterLemma:initial;
 const attempt=candidate===3?{status:'CONFLICT',source:'theory',message:'a ∧ c inconsistent, lemma (!a v !c)',trail:[-2,1,3],levels:[0,0,1],clause:[-1,-3],core:[1,3],lemma:[-1,-3],propagated:[]} : base;
 const options=learnt?[]:[3,-3,4,-4].map(lit=>({literal:lit,status:'CONFLICT',reason:'Theorie inconsistentă'}));
 return {ok:true,json:async()=>({mode:'MANUAL',definitions:defs,clauses:[[1,2],[3,4],[-2]],base,attempt,options})};
}
const context={document,fetch,navigator:{clipboard:{writeText:async()=>{}}},console,clearInterval,setInterval,setTimeout};
const html=fs.readFileSync(__dirname+'/web/index.html','utf8'),js=html.match(/<script>([\s\S]*?)<\/script>/)[1];
vm.createContext(context);vm.runInContext(js,context);
(async()=>{
 assert($('source').value.includes('atom a: x+1 > 0'));
 await $('manual-run').onclick();
 assert.equal($('manual-panel').hidden,false);
 assert.equal($('manual-choices').children.length,4);
 $('last').onclick(); assert.equal($('context-items').children.length,2); $('first').onclick();
 assert($('manual-choices').children[2].children.some(e=>e.className==='manual-opts'));
 assert.equal($('m-step').textContent,'1 / 3');
 await vm.runInContext('manualPick(3)',context);
 assert.equal($('manual-learn').hidden,false);
 assert($('manual-feedback').textContent.includes('Ramura NU a fost acceptată'));
 assert.equal(vm.runInContext('manualChoices.length',context),0);
 assert.equal(vm.runInContext('manualPending.join(",")',context),'-1,-3');
 await $('manual-learn').onclick();
 assert.equal(vm.runInContext('manualLemmas.length',context),1);
 assert.equal($('learn-count').textContent,'1');
 assert(requests.some(x=>x.headers['X-Manual-Lemmas']==='-1,-3'));
 assert.equal(vm.runInContext('manualPending.join(",")',context),'2,-4');
 await $('manual-learn').onclick();
 assert.equal(vm.runInContext('manualLemmas.length',context),2);
 $('first').onclick();assert.equal($('learn-count').textContent,'0');
 $('last').onclick();assert.equal($('learn-count').textContent,'2');
 assert.equal($('status').textContent,'UNSAT');
 console.log('PASS: manual decisions, red conflict, core lemma, apply, propagation, Next/Previous');
})().catch(e=>{console.error(e);process.exitCode=1});
