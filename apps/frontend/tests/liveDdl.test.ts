import test from 'node:test';
import assert from 'node:assert/strict';
import { columnDefinitions, definitionProblem, editColumnSql, addColumnSql, dropColumnSql, createDatabaseSql, columnNameProblem, charsetProblem } from '../src/services/liveDdl.ts';

const create = "CREATE TABLE `apartments` (\n  `id` bigint unsigned NOT NULL AUTO_INCREMENT,\n  `we``ird` varchar(9) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin DEFAULT 'a,b' COMMENT 'x;y',\n  `updated_at` timestamp NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,\n  PRIMARY KEY (`id`),\n  UNIQUE KEY `apartments_unit_unique` (`we``ird`)\n) ENGINE=InnoDB AUTO_INCREMENT=78 DEFAULT CHARSET=utf8mb4";

test('column definitions are read exactly from SHOW CREATE TABLE, keys excluded', () => {
  const defs = columnDefinitions(create);
  assert.deepEqual([...defs.keys()], ['id', 'we`ird', 'updated_at']);
  assert.equal(defs.get('id'), 'bigint unsigned NOT NULL AUTO_INCREMENT');
  assert.equal(defs.get('we`ird'), "varchar(9) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin DEFAULT 'a,b' COMMENT 'x;y'");
  assert.equal(defs.get('updated_at'), 'timestamp NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP');
});

test('a rename keeps the server definition; a definition change restates only the edited text', () => {
  const before = 'timestamp NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP';
  assert.equal(editColumnSql('MySQL', 't', 'updated_at', 'updated_at', before, before), '');
  assert.equal(editColumnSql('MySQL', 't', 'updated_at', 'changed_at', before, before), 'ALTER TABLE `t` RENAME COLUMN `updated_at` TO `changed_at`;');
  assert.equal(editColumnSql('MariaDB', 't', 'a`b', 'a`b', 'int NULL', 'bigint NULL'), 'ALTER TABLE `t` CHANGE COLUMN `a``b` `a``b` bigint NULL;');
  assert.equal(addColumnSql('MySQL', 't', 'note', 'VARCHAR(20) NULL', { kind: 'after', column: 'id' }), 'ALTER TABLE `t` ADD COLUMN `note` VARCHAR(20) NULL AFTER `id`;');
  assert.equal(addColumnSql('MySQL', 't', 'n', 'INT', { kind: 'first' }), 'ALTER TABLE `t` ADD COLUMN `n` INT FIRST;');
  assert.equal(dropColumnSql('MySQL', 't', 'n'), 'ALTER TABLE `t` DROP COLUMN `n`;');
  assert.equal(createDatabaseSql('MySQL', 'shop', 'utf8mb4', 'utf8mb4_0900_ai_ci'), 'CREATE DATABASE `shop` CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci;');
});

test('a definition cannot end the statement or hide text; quoted text may contain anything', () => {
  for (const bad of ['INT; DROP TABLE t', 'INT -- x', 'INT /* x */', 'INT # x', 'INT `x`', "VARCHAR(9) DEFAULT 'open", '']) assert.notEqual(definitionProblem(bad), '', bad);
  for (const good of ["VARCHAR(9) DEFAULT 'a;b' COMMENT 'it''s -- fine'", 'INT UNSIGNED NOT NULL DEFAULT 0', "ENUM('a','b') NULL"]) assert.equal(definitionProblem(good), '', good);
  assert.notEqual(columnNameProblem(''), '');
  assert.notEqual(columnNameProblem('x\u0000'), '');
  assert.equal(columnNameProblem('a`b'), '');
  assert.notEqual(charsetProblem('utf8mb4; DROP'), '');
  assert.equal(charsetProblem('utf8mb4_bin'), '');
});
