import { test } from 'node:test';
import assert from 'node:assert/strict';
import { csvRecords } from '../src/services/csvImport.ts';
import { insertRecords } from '../src/services/recordMutations.ts';
import type { Table } from '../src/types/index.ts';
const table: Table = { name: 'raw', columns: [{name:'id',type:'bigint',nullable:false,primary:true,autoIncrement:true},{name:'value',type:'varchar(100)',nullable:false}],rows:[] };
test('H10 CSV preserves exact numeric strings, multiline fields, empty strings and quotes',()=>{
 const rows = csvRecords('id,value\r\n9007199254740993,"line 1\nline ""2"""\r\n9007199254740994,',table);
 assert.deepEqual(rows,[{id:'9007199254740993',value:'line 1\nline "2"'},{id:'9007199254740994',value:''}]);
 const target = structuredClone(table); insertRecords({name:'db',tables:[target]},target,rows,'MySQL'); assert.equal(target.rows[0]!.id,'9007199254740993');
});
test('H10 CSV rejects duplicate headers, malformed quotes and uneven records',()=>{
 for(const csv of ['id,id\n1,2','id,value\n1','id,value\n1,"unclosed','id,value\n1,"closed"tail','id,value\n1,un"quoted']) assert.throws(()=>csvRecords(csv,table));
});
test('H10 import constraint failures leave the complete table unchanged',()=>{
 const target = structuredClone(table), db={name:'db',tables:[target]};
 assert.throws(()=>insertRecords(db,target,csvRecords('id,value\n1,valid\n1,duplicate',target),'MySQL'));
 assert.equal(target.rows.length,0);
});
