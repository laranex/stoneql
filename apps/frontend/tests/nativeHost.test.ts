import test from 'node:test';
import assert from 'node:assert/strict';
import {bootstrapNativeHost, confirmNativeRendered, type NativeInvoker} from '../src/services/nativeHost.ts';

function native(mutate?: (command:string,payload:any)=>any) {
  const calls:string[]=[];
  const invoker:NativeInvoker={async invoke<T>(command,args):Promise<T>{
    calls.push(command);
    const input=JSON.parse(args.message);
    let payload=command==='host_info'?{runtime:'desktop',appVersion:'0.1.0',sessionId:'owned_session',nonce:input.payload.nonce,simulated:true}
      :{simulated:true,values:input.payload.values.map((value:any)=>Object.fromEntries(Object.entries(value).reverse()))};
    if(mutate) payload=mutate(command,payload);
    return {version:1,requestId:input.requestId,payload} as T;
  }};
  return {global:{__TAURI__:{core:invoker}},calls};
}
test('ordinary browser startup never tries native IPC',async()=>{
  assert.deepEqual(await bootstrapNativeHost({}),{kind:'browser'});
  await confirmNativeRendered();
});
test('active bridge validates actual protocol values independent of JSON property order',async()=>{
  const fixture=native();const state=await bootstrapNativeHost(fixture.global);
  assert.equal(state.kind,'desktop');
  await confirmNativeRendered();
  assert.deepEqual(fixture.calls,['host_info','contract_probe','contract_probe']);
  await bootstrapNativeHost({});
});
test('lossy or foreign native replies fail closed and never acknowledge readiness',async()=>{
  for(const mutate of [
    (command:string,payload:any)=>command==='contract_probe'?{...payload,values:[{kind:'integer',text:18446744073709551615}]}:payload,
    (command:string,payload:any)=>command==='host_info'?{...payload,nonce:'foreign_nonce'}:payload,
  ]){
    const fixture=native(mutate);assert.deepEqual(await bootstrapNativeHost(fixture.global),{kind:'unavailable'});
    const before=fixture.calls.length;await confirmNativeRendered();assert.equal(fixture.calls.length,before);
  }
  await bootstrapNativeHost({});
});
