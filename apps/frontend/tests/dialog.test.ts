import {test} from 'node:test';
import assert from 'node:assert/strict';
import {dialog} from '../src/directives/dialog.ts';
test('H7 shared dialog sets initial focus, traps Tab, handles Escape and restores its trigger',async()=>{
 const listeners = new Map<string,Function>();
 const document:any={defaultView:{},activeElement:null,addEventListener:(name:string,fn:Function)=>listeners.set(name,fn),removeEventListener:(name:string,fn:Function)=>{if(listeners.get(name)===fn)listeners.delete(name);}};
 const control=()=>({tabIndex:0,isConnected:true,getClientRects:()=>[{}],focus(){document.activeElement=this;}});
 const trigger=control(), cancel=control(), confirm=control();document.activeElement=trigger;
 const element:any={ownerDocument:document,isConnected:true,tabIndex:0,querySelector:()=>cancel,querySelectorAll:()=>[cancel,confirm],contains:(node:any)=>[cancel,confirm,element].includes(node),focus(){document.activeElement=this;}};
 let closed=0;
 (dialog as any).mounted(element,{value:{close:()=>closed++}});await Promise.resolve();assert.equal(document.activeElement,cancel);
 const key=(value:string,shift=false)=>{let prevented=false;listeners.get('keydown')!({key:value,shiftKey:shift,preventDefault(){prevented=true},stopImmediatePropagation(){}});return prevented;};
 assert.equal(key('Tab',true),true);assert.equal(document.activeElement,confirm);
 assert.equal(key('Tab'),true);assert.equal(document.activeElement,cancel);
 document.activeElement=trigger;listeners.get('focusin')!({target:trigger});assert.equal(document.activeElement,cancel);
 assert.equal(key('Escape'),true);assert.equal(closed,1);
 (dialog as any).beforeUnmount(element);await Promise.resolve();assert.equal(document.activeElement,trigger);assert.equal(listeners.size,0);
});
