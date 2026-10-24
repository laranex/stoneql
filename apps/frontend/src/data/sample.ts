import { seedObjects } from './objects.ts';
import type { Connection, Database, Engine } from '../types';
export const connections: Connection[] = [
  {
    id: 'local',
    name: 'Local development',
    engine: 'MySQL',
    host: '127.0.0.1',
    port: 3306,
    user: 'root',
    database: 'acme_store',
    transport: { type: 'direct' },
    color: '#378777',
  },
  {
    id: 'staging',
    name: 'Staging server',
    engine: 'PostgreSQL',
    host: 'staging.internal',
    port: 5432,
    user: 'admin',
    database: 'acme_store',
    transport: { type: 'direct' },
    color: '#bc8b49',
  },
];
const names = [
  'Olivia Rhye',
  'Phoenix Baker',
  'Lana Steiner',
  'Demi Wilkinson',
  'Drew Cano',
  'Natali Craig',
  'Orlando Diggs',
  'Andi Lane',
  'Kate Morrison',
  'Koray Okumus',
  'Candice Wu',
  'Ava Thompson',
  'Liam Wilson',
  'Noah Martin',
  'Emma Davis',
  'James Anderson',
  'Sophia Garcia',
  'Lucas Brown',
  'Mia Robinson',
  'Ethan Clark',
];
export function createDatabases(engine: Engine = 'MySQL'): Database[] {
  const users = Array.from({ length: 128 }, (_, i) => ({
    id: i + 1,
    name: names[i % names.length]!,
    email: `${names[i % names.length]!.toLowerCase().replace(' ', '.')}${i >= 20 ? i : ''}@example.com`,
    permissions: i % 4 === 0 ? 'read,write' : 'read',
    role: i % 11 === 0 ? 'admin' : i % 4 === 0 ? 'editor' : 'customer',
    status: i % 7 === 4 ? 'inactive' : 'active',
    created_at: `2026-10-${String(24 - (i % 23)).padStart(2, '0')} ${String(9 + (i % 9)).padStart(2, '0')}:${String((i * 7) % 60).padStart(2, '0')}:00`,
  }));
  const databases: Database[] = [
    {
      name: 'acme_store',
      tables: [
        {
          name: 'users',
          columns: [
            { name: 'id', type: 'int unsigned', nullable: false, primary: true },
            { name: 'name', type: 'varchar(255)', nullable: false },
            { name: 'email', type: 'varchar(255)', nullable: false, unique: true },
            { name: 'role', type: 'varchar(32)', nullable: false, default: 'customer' },
            {
              name: 'status',
              type:
                engine === 'PostgreSQL'
                  ? 'user_status'
                  : engine === 'SQLite'
                    ? 'text'
                    : "enum('active','inactive','suspended')",
              nullable: false,
              default: 'active',
              ...(engine === 'PostgreSQL'
                ? { enumName: 'user_status', enumValues: ['active', 'inactive', 'suspended'] }
                : {}),
            },
            {
              name: 'permissions',
              type: ['MySQL', 'MariaDB'].includes(engine) ? "set('read','write','export')" : 'text',
              nullable: true,
              default: 'read',
            },
            { name: 'created_at', type: 'datetime', nullable: false, default: 'CURRENT_TIMESTAMP' },
          ],
          rows: users,
        },
        {
          name: 'orders',
          columns: [
            { name: 'id', type: 'int unsigned', nullable: false, primary: true },
            { name: 'user_id', type: 'int unsigned', nullable: false, foreign: 'users.id' },
            { name: 'total', type: 'decimal(10,2)', nullable: false },
            { name: 'status', type: 'varchar(32)', nullable: false, indexed: true },
          ],
          rows: Array.from({ length: 246 }, (_, i) => ({
            id: i + 1,
            user_id: (i % 128) + 1,
            total: Number((29.99 + i * 3.7).toFixed(2)),
            status: ['completed', 'processing', 'pending'][i % 3]!,
          })),
        },
        {
          name: 'order_items',
          columns: [
            { name: 'id', type: 'int', primary: true, nullable: false },
            { name: 'order_id', type: 'int', nullable: false, foreign: 'orders.id' },
            { name: 'product_id', type: 'int', nullable: false, foreign: 'products.id' },
            { name: 'quantity', type: 'int', nullable: false },
          ],
          rows: Array.from({ length: 412 }, (_, i) => ({
            id: i + 1,
            order_id: (i % 246) + 1,
            product_id: (i % 48) + 1,
            quantity: (i % 4) + 1,
          })),
        },
        {
          name: 'products',
          columns: [
            { name: 'id', type: 'int', primary: true, nullable: false },
            { name: 'name', type: 'varchar(255)', nullable: false },
            { name: 'price', type: 'decimal(10,2)', nullable: false },
            { name: 'category_id', type: 'int', nullable: false, foreign: 'categories.id' },
          ],
          rows: Array.from({ length: 48 }, (_, i) => ({
            id: i + 1,
            name:
              ['Everyday tote', 'Ceramic mug', 'Linen notebook', 'Desk organizer'][i % 4]! +
              ` ${i + 1}`,
            price: 12 + i * 2,
            category_id: (i % 5) + 1,
          })),
        },
        {
          name: 'categories',
          columns: [
            { name: 'id', type: 'int', primary: true, nullable: false },
            { name: 'name', type: 'varchar(255)', nullable: false },
          ],
          rows: ['Accessories', 'Home', 'Stationery', 'Office', 'Gifts'].map((name, i) => ({
            id: i + 1,
            name,
          })),
        },
        {
          name: 'payments',
          columns: [
            { name: 'id', type: 'int', primary: true, nullable: false },
            { name: 'order_id', type: 'int', nullable: false, foreign: 'orders.id' },
            { name: 'amount', type: 'decimal(10,2)', nullable: false },
          ],
          rows: Array.from({ length: 219 }, (_, i) => ({
            id: i + 1,
            order_id: i + 1,
            amount: 29.99 + i,
          })),
        },
        {
          name: 'sessions',
          columns: [
            { name: 'id', type: 'int', primary: true, nullable: false },
            { name: 'user_id', type: 'int', nullable: false, foreign: 'users.id' },
          ],
          rows: [],
        },
      ],
    },
    {
      name: 'analytics',
      tables: [
        {
          name: 'events',
          columns: [
            { name: 'id', type: 'int', primary: true, nullable: false },
            { name: 'event', type: 'varchar(128)', nullable: false },
          ],
          rows: Array.from({ length: 75 }, (_, i) => ({
            id: i + 1,
            event: ['page_view', 'purchase', 'sign_in'][i % 3]!,
          })),
        },
      ],
    },
    {
      name: 'information_schema',
      tables: [
        {
          name: 'SCHEMATA',
          columns: [{ name: 'SCHEMA_NAME', type: 'varchar(64)', nullable: false }],
          rows: [{ SCHEMA_NAME: 'acme_store' }, { SCHEMA_NAME: 'analytics' }],
        },
      ],
    },
  ];
  for (const db of databases)
    for (const table of db.tables)
      for (const column of table.columns)
        if (column.primary) column.autoIncrement = table.name !== 'order_items';
  if (engine === 'PostgreSQL' || engine === 'SQLite')
    for (const db of databases)
      for (const table of db.tables)
        for (const column of table.columns) {
          column.type = column.type.replace(/ unsigned$/i, '');
          if (engine === 'SQLite') {
            if(column.primary && column.autoIncrement) column.type='integer';
            if (/^(varchar|char|datetime)/i.test(column.type)) column.type = 'text';
          } else if (column.type === 'datetime') column.type = 'timestamp';
        }
  seedObjects(databases[0]!,engine);
  return databases;
}
