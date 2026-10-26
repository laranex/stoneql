import test from 'node:test';
import assert from 'node:assert/strict';
import {parseMaximumRss} from '../tools/resource-report.mjs';
test('resource profiles normalize macOS bytes and Linux KiB without mixing footprint fields',()=>{
 assert.equal(parseMaximumRss('  1048576  maximum resident set size\n  999 peak memory footprint\n','darwin'),1048576);
 assert.equal(parseMaximumRss('Maximum resident set size (kbytes): 1024\n','linux'),1048576);
});
test('resource profiles reject missing, duplicate, invalid and unsupported RSS observations',()=>{
 for(const text of ['', '0 maximum resident set size', '-1 maximum resident set size', '9007199254740992 maximum resident set size','10 maximum resident set size\n20 maximum resident set size'])assert.throws(()=>parseMaximumRss(text,'darwin'));
 assert.throws(()=>parseMaximumRss('Maximum resident set size (kbytes): 9007199254740991','linux'));
 assert.throws(()=>parseMaximumRss('1 maximum resident set size','win32'));
});
