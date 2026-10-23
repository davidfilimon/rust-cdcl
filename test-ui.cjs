'use strict';
const fs=require('fs'),vm=require('vm'),assert=require('assert');
class Element {
 constructor(tag='div'){this.tagName=tag.toUpperCase();this.children=[];this.value='';this.textContent='';this.innerHTML='';this.disabled=false;this.hidden=false;this.className='';this.title='';this.style={};this.classList={toggle:(x,y)=>{if(x==='fullscreen'){this.fullscreen=!this.fullscreen;return this.fullscreen;}},add:()=>{},remove:()=>{},contains:x=>this.fullscreen||false};}
 append(...nodes){this.children.push(...nodes)}
 replaceChildren(...nodes){this.children=nodes;this.textContent=''}
 addEventListener(name,fn){this['on'+name]=fn}
 querySelectorAll(s){return this.children.filter(c=>s==='.example'||s==='button'?c.tagName==='BUTTON':false)}
}
let elements=new Map();const document={getElementById(id){if(!elements.has(id))elements.set(id,new Element());return elements.get(id)},createElement(tag){return new Element(tag)},querySelectorAll(selector){if(selector==='.example')return elements.get('examples').children;return[]},addEventListener(){},activeElement:null,body:new Element('body')};
const data={status:'UNSAT',mode:'SMT',definitions:[{name:'a',expression:'x+1 > 0'},{name:'b',expression:'x+y > 0'},{name:'c',expression:'x < 0'},{name:'d',expression:'x+y > 4'}],clauses:[[1,2],[3,4],[-2]],model:{},atoms:{},events:[
 {kind:'start',source:'smt',message:'start',iteration:0,level:0,trail:[],levels:[],clause:[],decisions:0,conflicts:0},
 {kind:'propagation',source:'cdcl',message:'b false',iteration:1,level:0,trail:[-2],levels:[0],clause:[-2],decisions:0,conflicts:0},
 {kind:'propagation',source:'cdcl',message:'a true',iteration:1,level:0,trail:[-2,1],levels:[0,0],clause:[1,2],decisions:0,conflicts:0},
 {kind:'theory_conflict',source:'theory',message:'Core inconsistent',iteration:1,level:0,trail:[-2,1,3,4],levels:[0,0,0,0],clause:[],decisions:0,conflicts:0},
 {kind:'theory_lemma',source:'theory',message:'Learned valid lemma',iteration:1,level:0,trail:[-2,1,3,4],levels:[0,0,0,0],clause:[2,-4],decisions:0,conflicts:0},
 {kind:'unsat',source:'result',message:'no model',iteration:2,level:0,trail:[],levels:[],clause:[],decisions:0,conflicts:0}
]};
const html=fs.readFileSync(__dirname+'/web/index.html','utf8');const js=html.match(/<script>([\s\S]*?)<\/script>/)[1];
let clipboard='';const context={document,fetch:async()=>({ok:true,json:async()=>data}),navigator:{clipboard:{writeText:async text=>{clipboard=text}}},clearInterval,setInterval,setTimeout,console};vm.createContext(context);vm.runInContext(js,context);
const $=id=>document.getElementById(id);
(async()=>{
 assert($('source').value.includes('atom a: x+1 > 0'),'Slide 50 preset missing');
 await $('run').onclick();
 assert.equal($('m-step').textContent,'1 / 6');
 assert.equal($('learn-count').textContent,'0');
 $('next').onclick();assert.equal($('context-items').children.length,1);
 $('last').onclick();assert.equal($('status').textContent,'UNSAT');
 $('prev').onclick();assert.equal($('learn-count').textContent,'1');
 assert($('formula').children.some(n=>n.className?.includes('lemma')),'learned clause absent from formula');
 assert($('learning-proof').textContent.includes('¬d'),'proof missing');
 await $('copy-lemmas').onclick();assert.equal(clipboard,'clause b !d');
 $('add-lemmas').onclick();assert($('source').value.includes('clause b !d'),'lemma not appended to SMT');
 assert.equal($('status').textContent,'NEPORNIT','should reset replay');
 console.log('PASS: presets, steps, context, learned clause, proof, export, reset');
})().catch(err=>{console.error(err);process.exitCode=1});
