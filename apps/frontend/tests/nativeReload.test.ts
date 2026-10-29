import test from 'node:test';
import assert from 'node:assert/strict';
import {bootstrapNativeHost, invalidateNativeSession, nativeDatabase, type NativeInvoker} from '../src/services/nativeHost.ts';

function native(rotate = true) {
  const calls:{command:string,payload:any}[]=[];
  let session=0;
  const invoker:NativeInvoker={async invoke<T>(command,args):Promise<T>{
    const input=JSON.parse(args.message);
    calls.push({command,payload:input.payload});
    const payload=command==='host_info'?{runtime:'desktop',appVersion:'0.1.0',sessionId:`session_${rotate?++session:1}`,nonce:input.payload.nonce,simulated:true}
      :{simulated:true,values:input.payload.values};
    return {version:1,requestId:input.requestId,payload} as T;
  }};
  return {global:{__TAURI__:{core:invoker}},calls};
}
test('confirmed reload rotates the native session with a fresh nonce and leaves the old one stale',async()=>{
  const fixture=native();
  assert.equal((await bootstrapNativeHost(fixture.global)).kind,'desktop');
  await invalidateNativeSession();
  const hostInfo=fixture.calls.filter((call)=>call.command==='host_info');
  assert.equal(hostInfo.length,2);
  assert.notEqual(hostInfo[0]!.payload.nonce,hostInfo[1]!.payload.nonce);
  assert.match(hostInfo[1]!.payload.nonce,/^reload_/);
  // Nothing more is sent on the invalidated session before the page reloads.
  const before=fixture.calls.length;
  await assert.rejects(nativeDatabase('database_status',{handle:'h'}),{code:'UNSUPPORTED_CAPABILITY'});
  assert.equal(fixture.calls.length,before);
  await bootstrapNativeHost({});
});
test('a reload is not confirmed when the host did not rotate the session',async()=>{
  const fixture=native(false);
  await bootstrapNativeHost(fixture.global);
  await assert.rejects(invalidateNativeSession(),/not rotated/);
  await bootstrapNativeHost({});
  await assert.rejects(invalidateNativeSession(),/unavailable/);
});
