import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createDatabases } from '../src/data/sample.ts';
import { tableIndexes, saveIndex, dropIndex, validateIndex, rowIndexError } from '../src/services/indexes.ts';
import { saveView, previewView, dropView, copyMockTable, saveRoutine, saveTrigger, saveEvent, dropDatabaseObject, simulateRoutine } from '../src/services/databaseObjects.ts';
import { insertRecords, updateRecord, deleteRecords } from '../src/services/recordMutations.ts';
import { createColumnDraft, applyColumnDefinition } from '../src/services/columnDefinition.ts';
import { objectSections, relations } from '../src/services/objectCapabilities.ts';
import { exportSummary } from '../src/services/exportService.ts';
import { defaultExport } from '../src/types/export.ts';
import type { Table, Engine } from '../src/types/index.ts';
import type { TableIndex, Routine } from '../src/types/objects.ts';
const index=(name:string,columns:string[],kind:TableIndex['kind']='normal',method:TableIndex['method']=kind==='fulltext'?'fulltext':'btree'):TableIndex=>({name,kind,method,columns:columns.map(column=>({column,order:'ASC'}))});
const fixture=():Table=>({name:'pairs',columns:[{name:'a',type:'bigint',nullable:false},{name:'b',type:'varchar(20)',nullable:false}],rows:[{a:'9007199254740993',b:'one'},{a:'9007199254740993',b:'two'}]});
test('named indexes migrate legacy flags; create/edit/drop preserve order, directions and column flags',()=>{
 const table=fixture();saveIndex(table,index('pair_lookup',['a','b']),'MySQL');
 const edited=index('renamed',['b','a']);edited.columns[0]!.order='DESC';saveIndex(table,edited,'MySQL','pair_lookup');
 assert.deepEqual(tableIndexes(table)[0],edited);assert.equal(table.columns[0]!.indexed,true);dropIndex(table,'renamed');assert.equal(tableIndexes(table).length,0);assert.equal(table.columns[0]!.indexed,false);
 const users=createDatabases()[0]!.tables[0]!;assert.equal(tableIndexes(users)[0]!.kind,'primary');assert.throws(()=>dropIndex(users,'PRIMARY'),/generated/);
});
test('composite primary/unique keys enforce tuples atomically through shared CRUD without making each part unique',()=>{
 const table=fixture(),db={name:'db',tables:[table]};saveIndex(table,index('pair_pk',['a','b'],'primary'),'PostgreSQL');
 assert.equal(rowIndexError(table,{a:'9007199254740993',b:'three'}),undefined);
 insertRecords(db,table,[{a:'9007199254740993',b:'three'}],'PostgreSQL');
 const before=JSON.stringify(table.rows);assert.throws(()=>insertRecords(db,table,[{a:'1',b:'four'},{a:'9007199254740993',b:'one'}],'PostgreSQL'),/exists/);assert.equal(JSON.stringify(table.rows),before);
 assert.throws(()=>updateRecord(db,table,table.rows[1]!,{b:'one'},'PostgreSQL'),/exists/);
 updateRecord(db,table,table.rows[1]!,{a:'1',b:'one'},'PostgreSQL');assert.equal(table.rows[1]!.a,'1');
});
test('index validation rejects duplicate/null key data, duplicate parts, unsupported methods and dependent key removal',()=>{
 const table=fixture();assert.throws(()=>saveIndex(table,index('bad',['a'],'unique'),'MySQL'),/exists/);
 assert.throws(()=>saveIndex(table,index('dupe',['a','a']),'MySQL'),/distinct/);
 assert.throws(()=>saveIndex(table,index('pg_hash',['a','b'],'normal','hash'),'PostgreSQL'),/one key/);
 const db=createDatabases()[0]!,users=db.tables[0]!;
 saveIndex(users,index('id_lookup',['id']),'MySQL');assert.throws(()=>dropIndex(users,'PRIMARY',db),/foreign-key/);
 assert.throws(()=>saveIndex(users,index('PRIMARY',['name'],'primary'),'MySQL','PRIMARY',db),/generated|foreign-key|exists/);
});
test('composite UNIQUE allows NULL tuples and column renames preserve all ordered index parts',()=>{
 const table=fixture();table.columns[1]!.nullable=true;table.rows=[{a:'1',b:null},{a:'1',b:null}];
 saveIndex(table,index('pair_uq',['a','b'],'unique'),'SQLite');assert.equal(rowIndexError(table,{a:'1',b:null}),undefined);
 const draft=createColumnDraft('SQLite',table.columns[1]);draft.name='renamed';applyColumnDefinition(table,draft,'SQLite','b');assert.equal(tableIndexes(table)[0]!.columns[1]!.column,'renamed');
});
test('Fulltext/Spatial and PostgreSQL index methods respect capabilities and available data types',()=>{
 for(const engine of ['MySQL','MariaDB'] as Engine[]){const products=createDatabases(engine)[0]!.tables.find(t=>t.name==='products')!;
 saveIndex(products,index('search_name',['name'],'fulltext'),engine);saveIndex(products,index('geo',['location'],'spatial','rtree'),engine);
 assert.throws(()=>saveIndex(products,index('bad_geo',['name'],'spatial','rtree'),engine),/geometry/);}
 const products=createDatabases('PostgreSQL')[0]!.tables.find(t=>t.name==='products')!;
 for(const [method,column] of [['btree','name'],['hash','name'],['gist','location'],['spgist','location'],['gin','metadata'],['brin','id']] as const) saveIndex(products,index(`idx_${method}`,[column],'normal',method),'PostgreSQL');
 assert.throws(()=>saveIndex(products,index('bad_gin',['name'],'normal','gin'),'PostgreSQL'),/GIN/);
 assert.ok(validateIndex(fixture(),index('full',['b'],'fulltext'),'SQLite').length);assert.ok(validateIndex(fixture(),index('hash',['a'],'normal','hash'),'MySQL').length);
});
test('all engines create/edit/browse/drop views; materialized views are PostgreSQL-only and data is read-only',()=>{
 for(const engine of ['MySQL','MariaDB','PostgreSQL','SQLite'] as Engine[]){const db=createDatabases(engine)[0]!;
 let view=saveView(db,{name:'new_view',kind:'view',definition:'SELECT id, name FROM users LIMIT 2;'},engine);previewView(db,view);assert.equal(view.rows.length,2);
 assert.throws(()=>insertRecords(db,view,[{id:'999'}],engine),/read-only/);assert.throws(()=>deleteRecords(db,view,[view.rows[0]!]),/read-only/);
 view=saveView(db,{name:'renamed_view',kind:'view',definition:'SELECT id FROM users LIMIT 1;'},engine,'new_view');assert.equal(view.rows.length,0);previewView(db,view);assert.equal(view.rows.length,1);dropView(db,'renamed_view');assert.ok(!relations(db).some(t=>t.name==='renamed_view'));
 if(engine!=='PostgreSQL')assert.throws(()=>saveView(db,{name:'material',kind:'materialized-view',definition:'SELECT id FROM users;'},engine),/PostgreSQL/);
 }
 const db=createDatabases('PostgreSQL')[0]!,view=db.views!.find(v=>v.kind==='materialized-view')!;previewView(db,view);assert.ok(view.refreshedAt);saveIndex(view,index('snapshot_id',['id'],'unique'),'PostgreSQL');
 const old=JSON.stringify(view.rows);view.definition='SELECT COUNT(*) FROM missing';assert.throws(()=>previewView(db,view));assert.equal(JSON.stringify(view.rows),old);
});
test('routines have independent parameter definitions and validated, explicitly simulated calls',()=>{
 const routine:Routine={name:'find',kind:'procedure',parameters:[{name:'identifier',type:'BIGINT',mode:'IN'}],returns:'',language:'SQL',definition:'SELECT 1;'};
 const db=createDatabases()[0]!;saveRoutine(db,routine,'MySQL');assert.match(simulateRoutine(routine,{identifier:'9007199254740993'},'MySQL'),/Body not executed/);
 assert.throws(()=>simulateRoutine(routine,{identifier:'broken'},'MySQL'),/identifier/);
 const fn={...routine,kind:'function' as const,returns:'INT',name:'calc'};saveRoutine(db,fn,'MySQL');saveRoutine(db,{...fn,returns:'BIGINT'},'MySQL','calc');assert.equal(db.routines!.find(r=>r.name==='calc')!.returns,'BIGINT');dropDatabaseObject(db,'routines','calc','MySQL');
 assert.throws(()=>saveRoutine(db,routine,'SQLite'),/not supported/);assert.throws(()=>saveRoutine(db,{...fn,returns:''},'MySQL'),/return type/);
 assert.throws(()=>saveRoutine(db,{...fn,parameters:[{...routine.parameters[0]!,mode:'OUT'}]},'MySQL'),/input-only/);
});
test('database triggers and events validate targets, timings, schedules and unsupported engine operations',()=>{
 const db=createDatabases()[0]!;saveTrigger(db,{name:'audit',table:'users',timing:'AFTER',event:'INSERT',definition:'SET @seen = 1;'},'MySQL');dropDatabaseObject(db,'triggers','audit','MySQL');
 assert.throws(()=>saveTrigger(db,{name:'bad',table:'missing',timing:'AFTER',event:'INSERT',definition:'SELECT 1;'},'SQLite'),/existing/);
 assert.throws(()=>saveTrigger(db,{name:'bad',table:'active_customers',timing:'BEFORE',event:'INSERT',definition:'SELECT 1;'},'SQLite'),/INSTEAD OF/);
 saveTrigger(db,{name:'view_write',table:'active_customers',timing:'INSTEAD OF',event:'INSERT',definition:'SELECT 1;'},'SQLite');
 saveEvent(db,{name:'daily',schedule:'EVERY 2 DAY',enabled:false,definition:'DELETE FROM sessions;'},'MariaDB');saveEvent(db,{name:'daily',schedule:'EVERY 3 HOUR',enabled:true,definition:'SELECT 1;'},'MariaDB','daily');assert.equal(db.events!.find(e=>e.name==='daily')!.enabled,true);
 assert.throws(()=>saveEvent(db,{name:'bad',schedule:'EVERY 0 DAY',enabled:true,definition:'SELECT 1;'},'MySQL'),/positive/);assert.throws(()=>saveEvent(db,{name:'bad',schedule:'EVERY 1 DAY',enabled:true,definition:'SELECT 1;'},'PostgreSQL'),/MySQL/);
 assert.deepEqual(objectSections('SQLite'),['Triggers']);assert.deepEqual(objectSections('PostgreSQL'),['Routines','Triggers']);
});
test('object export summaries count selected mock definitions rather than hardcoding zero',()=>{
 const db=createDatabases()[0]!,c=defaultExport();Object.assign(c,{scope:'database',mode:'custom',tables:['users'],views:true,routines:true,triggers:true,events:true});
 assert.deepEqual(exportSummary(c,db,'MySQL').objects,{views:1,routines:2,triggers:1,events:1});c.content='data';assert.deepEqual(exportSummary(c,db,'MySQL').objects,{views:0,routines:0,triggers:0,events:0});
});
test('indexed type changes reject incompatible Fulltext definitions without mutating the table',()=>{
 const table:Table={name:'texts',columns:[{name:'text',type:'varchar(50)',nullable:false}],rows:[{text:'123'}]};
 saveIndex(table,index('full',['text'],'fulltext'),'MySQL');const before=JSON.stringify(table),draft=createColumnDraft('MySQL',table.columns[0]);draft.dataType='INT';
 assert.throws(()=>applyColumnDefinition(table,draft,'MySQL','text'),/FULLTEXT/);assert.equal(JSON.stringify(table),before);
});
test('generated-key index restrictions distinguish PostgreSQL identity from MySQL and SQLite requirements',()=>{
 const table:Table={name:'identities',columns:[{name:'id',type:'integer',nullable:false,primary:true,autoIncrement:true}],rows:[]};
 const pg=structuredClone(table);dropIndex(pg,'PRIMARY',undefined,'PostgreSQL');assert.equal(pg.columns[0]!.primary,false);assert.equal(pg.columns[0]!.autoIncrement,true);
 for(const engine of ['MySQL','SQLite'] as Engine[]) assert.throws(()=>dropIndex(structuredClone(table),'PRIMARY',undefined,engine),/generated/);
});
test('explicit view exports distinguish SQL definitions from JSON snapshot rows and retain object filenames',()=>{
 const db=createDatabases()[0]!,c=defaultExport();c.tables=['active_customers'];
 let summary=exportSummary(c,db,'MySQL');assert.equal(summary.objects.views,1);assert.equal(summary.rows,0);assert.ok(summary.filename.includes('active_customers'));
 c.format='JSON';summary=exportSummary(c,db,'MySQL');assert.equal(summary.rows,25);assert.equal(summary.objects.views,0);
});
test('view renames preserve trigger target metadata; changing a definition clears refresh metadata',()=>{
 const db=createDatabases('SQLite')[0]!,view=db.views![0]!;
 saveTrigger(db,{name:'view_trigger',table:view.name,timing:'INSTEAD OF',event:'UPDATE',definition:'SELECT 1;'},'SQLite');
 const renamed=saveView(db,{name:'renamed_customers',kind:'view',definition:'SELECT id FROM users;'},'SQLite',view.name);assert.equal(db.triggers!.find(t=>t.name==='view_trigger')!.table,'renamed_customers');assert.equal(renamed.refreshedAt,undefined);
 dropView(db,'renamed_customers');assert.ok(!db.triggers!.some(t=>t.name==='view_trigger'));
});
test('PostgreSQL and SQLite index names use schema-level uniqueness while MySQL names remain table-local',()=>{
 for(const engine of ['PostgreSQL','SQLite','MySQL'] as Engine[]){const db=createDatabases(engine)[0]!,users=db.tables[0]!,products=db.tables.find(t=>t.name==='products')!;
 saveIndex(users,index('shared_lookup',['name']),engine,'',db);
 if(engine==='MySQL')saveIndex(products,index('shared_lookup',['name']),engine,'',db);else assert.throws(()=>saveIndex(products,index('shared_lookup',['name']),engine,'',db),/schema/);
 }
});
test('MySQL and MariaDB reserve PRIMARY for the primary key, while PostgreSQL accepts named primary constraints',()=>{
 for(const engine of ['MySQL','MariaDB'] as Engine[]){assert.throws(()=>saveIndex(fixture(),index('custom_pk',['a','b'],'primary'),engine),/named PRIMARY/);assert.throws(()=>saveIndex(fixture(),index('PRIMARY',['b'],'normal'),engine),/another name/);}
 saveIndex(fixture(),index('pair_primary',['a','b'],'primary'),'PostgreSQL');
});

test('table copying rejects view collisions atomically and generates unique schema-level index names',()=>{
  for(const engine of ['MySQL','MariaDB','PostgreSQL','SQLite'] as Engine[]) {
    const db=createDatabases(engine)[0]!,source=db.tables[0]!,before=JSON.stringify(db);
    assert.throws(()=>copyMockTable(db,source,'active_customers',true,engine),/already exists/);assert.equal(JSON.stringify(db),before);
    const copy=copyMockTable(db,source,'users_archive',true,engine);
    assert.equal(copy.rows.length,source.rows.length);assert.notEqual(copy.rows[0],source.rows[0]);assert.notEqual(copy.columns[0],source.columns[0]);
    if(engine==='PostgreSQL'||engine==='SQLite') {
      const names=db.tables.flatMap(table=>tableIndexes(table)).filter(index=>engine==='PostgreSQL'||index.kind!=='primary').map(index=>index.name);
      assert.equal(new Set(names).size,names.length);
    }
    const structure=copyMockTable(db,source,'users_empty',false,engine);assert.equal(structure.rows.length,0);assert.equal(source.rows.length,128);
  }
});
test('table copies preserve exact stored BIGINT and DECIMAL strings',()=>{
  const table:Table={name:'raw_copy',columns:[{name:'id',type:'bigint',nullable:false,primary:true,autoIncrement:false},{name:'amount',type:'decimal(30,12)',nullable:false}],rows:[{id:'9007199254740993',amount:'123456789012345678.123456789012'}]},db={name:'raw',tables:[table]};
  const copy=copyMockTable(db,table,'exact_copy',true,'PostgreSQL');assert.deepEqual(copy.rows,table.rows);
});
