import { test } from 'node:test';
import assert from 'node:assert/strict';
import { buildBrowseQuery } from '../src/services/browseQuery.ts';
import { filterRows, selectQuery } from '../src/services/mockTransport.ts';
import { searchRows } from '../src/services/search.ts';
import type { Engine, Table } from '../src/types/index.ts';
const table: Table={name:'odd table',columns:[{name:'value',type:'bigint',nullable:true},{name:'text',type:'text',nullable:false}],rows:[{value:null,text:''},{value:'9007199254740993',text:'foo_bar'},{value:'9007199254740992',text:'foo%bar'}]};
test('H2 one accurate preview plan covers engine quoting, filter, pagination and missing sort columns',()=>{
 for(const engine of ['MySQL','MariaDB','PostgreSQL','SQLite'] as Engine[]){
  const plan=buildBrowseQuery({engine,table:table.name,columns:table.columns,filter:'foo_',sort:'id',descending:false,offset:0,limit:1});
  assert.equal(plan.sort,''); assert.ok(!plan.sql.includes('ORDER BY')); assert.match(plan.sql,/foo!_/);
  assert.ok(plan.sql.includes(engine==='MySQL'||engine==='MariaDB'?'`odd table`':'"odd table"'));
  assert.deepEqual(selectQuery(plan.sql,[table],plan).rows,[table.rows[1]]);
 }
});
test('H3 exact sorting is stable, supports unsorted order and keeps NULL last in either direction',()=>{
 assert.deepEqual(filterRows(table.rows,'','value',false,'bigint').map(r=>r.value),['9007199254740992','9007199254740993',null]);
 assert.deepEqual(filterRows(table.rows,'','value',true,'bigint').map(r=>r.value),['9007199254740993','9007199254740992',null]);
 assert.deepEqual(filterRows(table.rows,'','',false),table.rows);
});
test('H9 NULL, LIKE, NOT LIKE, IN, NOT IN and BETWEEN use exact values without numeric precision loss',()=>{
 const run=(operator:any,value='')=>searchRows(table,[{column:operator.includes('LIKE')?'text':'value',operator,value}]);
 assert.equal(run('IS NULL').length,1); assert.equal(run('IS NOT NULL').length,2);
 assert.equal(run('LIKE','foo\\_bar').length,1); assert.equal(run('NOT LIKE','foo%').length,1);
 assert.equal(run('IN','9007199254740993').length,1); assert.equal(run('NOT IN','9007199254740993').length,1);
 assert.equal(run('BETWEEN','9007199254740993,9007199254740993').length,1);
 assert.throws(()=>run('BETWEEN','1'),/two/); assert.deepEqual(searchRows(table,[]),table.rows);
});

test('H9 explicit equality can find an empty string without matching SQL NULL',()=>{
 const target: Table={name:'strings',columns:[{name:'value',type:'text',nullable:true}],rows:[{value:null},{value:''},{value:'NULL'}]};
 assert.deepEqual(searchRows(target,[{column:'value',operator:'=',value:''}]),[{value:''}]);
});
