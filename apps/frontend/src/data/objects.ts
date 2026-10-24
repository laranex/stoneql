import { tableIndexes } from '../services/indexes.ts';
import type { Database, Engine } from '../types/index.ts';
import { previewView } from '../services/databaseObjects.ts';
/** Realistic definitions, not executable backend migrations. */
export function seedObjects(database: Database, engine: Engine) {
  const products = database.tables.find(t => t.name === 'products');
  if (products && ['MySQL', 'MariaDB'].includes(engine)) {
    products.columns.push({ name: 'location', type: 'point', nullable: false });
    products.rows.forEach((row, i) => row.location = `POINT(${i % 90} ${i % 45})`);
  }
  if (products && engine === 'PostgreSQL') {
    products.columns.push({ name: 'location', type: 'point', nullable: false }, { name: 'search_document', type: 'tsvector', nullable: false }, { name: 'metadata', type: 'jsonb', nullable: false });
    products.rows.forEach((row, i) => { row.location = `(${i % 90},${i % 45})`; row.search_document = "'product':1"; row.metadata = '{"source":"catalog"}'; });
  }
  if(engine==='PostgreSQL' || engine==='SQLite') for(const table of database.tables) table.indexes=tableIndexes(table).map(index=>({...index,name:index.kind==='primary' ? engine==='PostgreSQL'?`${table.name}_pkey`:'PRIMARY' : `${table.name}_${index.name}`}));
  database.views = [{ name: 'active_customers', kind: 'view', definition: "SELECT id, name, status FROM users WHERE status = 'active' LIMIT 25;", columns: [], rows: [] }];
  if (engine === 'PostgreSQL')
    database.views.push({ name: 'customer_snapshot', kind: 'materialized-view', definition: 'SELECT id, name FROM users LIMIT 25;', columns: [], rows: [] });
  for (const view of database.views)
    previewView(database, view, engine);
  database.routines = engine === 'SQLite' ? [] : [
    { name: 'customer_count', kind: 'function', parameters: [], returns: engine === 'PostgreSQL' ? 'bigint' : 'INT', language: engine === 'PostgreSQL' ? 'sql' : 'SQL', definition: engine === 'PostgreSQL' ? 'SELECT COUNT(*) FROM users;' : 'RETURN (SELECT COUNT(*) FROM users);' },
    { name: 'find_customer', kind: 'procedure', parameters: [{ name: 'customer_id', type: 'INT', mode: 'IN' }], returns: '', language: engine === 'PostgreSQL' ? 'plpgsql' : 'SQL', definition: engine === 'PostgreSQL' ? 'BEGIN\n  RAISE NOTICE \'Customer %\', customer_id;\nEND;' : 'BEGIN\n  SELECT * FROM users WHERE id = customer_id;\nEND' },
  ];
  if (engine === 'PostgreSQL')
    database.routines.push({ name: 'audit_user_changes', kind: 'function', parameters: [], returns: 'trigger', language: 'plpgsql', definition: 'BEGIN\n  RETURN NEW;\nEND;' });
  database.triggers = [{ name: 'users_before_update', table: 'users', timing: 'BEFORE', event: 'UPDATE', definition: engine === 'PostgreSQL' ? 'EXECUTE FUNCTION audit_user_changes();' : engine === 'SQLite' ? "BEGIN\n  SELECT RAISE(ABORT, 'Email is required') WHERE NEW.email = '';\nEND" : "BEGIN\n  IF NEW.email = '' THEN\n    SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'Email is required';\n  END IF;\nEND" }];
  database.events = ['MySQL', 'MariaDB'].includes(engine) ? [{ name: 'prune_expired_sessions', schedule: 'EVERY 1 DAY', enabled: false, definition: 'DELETE FROM sessions WHERE id < 0;' }] : [];
}
