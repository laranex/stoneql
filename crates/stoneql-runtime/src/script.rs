//! Conservative statement splitter and transaction policy for read-write scripts.
//!
//! Scripts run one statement at a time so every statement gets its own outcome. The splitter
//! understands each dialect's quoting and comments; it never rewrites statement text. The
//! policy refuses statements that would make the runtime's transaction accounting wrong.
use super::{execution::ExecutionLimits, read_only::Dialect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptError {
    Empty,
    TooLarge,
    TooManyStatements,
    /// Unterminated quote, identifier, comment or dollar-quoted body.
    Unterminated,
    /// BEGIN/COMMIT/ROLLBACK/autocommit changes belong to the transaction controls. Savepoints
    /// are not offered in this slice (SAVEPOINT/RELEASE/ROLLBACK TO are refused too).
    TransactionControl,
    /// Inside a MySQL/MariaDB lease only allowlisted statement kinds run (see `classify`); any
    /// other statement could commit implicitly or otherwise change the transaction.
    NotAllowedInTransaction,
    /// SQLite ATTACH/DETACH/VACUUM INTO reach files outside the granted database.
    FileAccess,
    /// MySQL/MariaDB PREPARE/EXECUTE/DEALLOCATE (and MariaDB EXECUTE IMMEDIATE) run SQL text the
    /// policy cannot see, for example a prepared COMMIT, DDL or autocommit change.
    PreparedStatement,
    /// A MySQL/MariaDB statement under a custom DELIMITER that contains a further statement
    /// outside a BEGIN … END body. The server would run it as a multi-statement unchecked. On
    /// SQLite, a statement that SQLite itself parses as more than one statement.
    MultipleStatements,
    /// MariaDB `SET STATEMENT … FOR <statement>` runs an inner statement the policy does not see.
    SetStatement,
    /// MySQL/MariaDB compound statements outside a stored program (CASE, IF, LOOP, WHILE,
    /// REPEAT, FOR, labelled blocks) can contain COMMIT, DDL or autocommit changes.
    CompoundStatement,
    /// MySQL/MariaDB `CALL`: a procedure can change the session's transaction state or run SQL
    /// the policy cannot see.
    Call,
}
impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Empty => "The script contains no statements.",
            Self::TooLarge => "The script exceeds the size limit.",
            Self::TooManyStatements => "The script exceeds the statement limit.",
            Self::Unterminated => "The script has an unterminated quote, comment or body.",
            Self::TransactionControl => {
                "Use the transaction controls instead of BEGIN, COMMIT, ROLLBACK, savepoint or autocommit statements."
            }
            Self::NotAllowedInTransaction => {
                "Inside a MySQL/MariaDB transaction only SELECT, WITH, INSERT, UPDATE, DELETE, REPLACE, VALUES, TABLE, SHOW, DESCRIBE, EXPLAIN, USE, SET @variable and CREATE/DROP TEMPORARY TABLE are accepted; run other statements outside the transaction."
            }
            Self::FileAccess => "ATTACH, DETACH and VACUUM INTO are not available for SQLite files.",
            Self::PreparedStatement => {
                "PREPARE, EXECUTE and DEALLOCATE are not available for MySQL/MariaDB scripts; run the statement directly."
            }
            Self::MultipleStatements => {
                "Only routine BEGIN … END bodies may contain ';' under a custom DELIMITER; separate other statements."
            }
            Self::SetStatement => {
                "SET STATEMENT … FOR is not available in read-write scripts; run the inner statement directly."
            }
            Self::CompoundStatement => {
                "CASE, IF, LOOP, WHILE, REPEAT, FOR and labelled blocks are not available outside a stored program."
            }
            Self::Call => {
                "CALL is not available in read-write MySQL/MariaDB scripts: a procedure can change the transaction state."
            }
        })
    }
}
impl std::error::Error for ScriptError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Statement {
    pub sql: String,
    /// Byte range of `sql` (the trimmed statement text) in the input.
    pub span: std::ops::Range<usize>,
    /// Every bare word, upper-cased, including words inside MySQL executable comments (and,
    /// for SET statements, quoted identifiers such as `` `autocommit` ``).
    pub words: Vec<String>,
    /// MySQL/MariaDB only: a `;` outside quotes, comments and BEGIN … END bodies occurred inside
    /// this statement (possible only under a custom DELIMITER).
    pub compound: bool,
    /// MySQL/MariaDB only: any `;` outside quotes and comments, including inside bodies.
    pub semicolon: bool,
    /// MySQL/MariaDB only: the statement starts with a label (`name:`), as a labelled compound
    /// statement does.
    pub label: bool,
}

/// Splits on the statement delimiter outside quotes, identifiers, comments and bodies.
/// MySQL/MariaDB honour the client `DELIMITER` directive (as the mysql CLI and phpMyAdmin do)
/// for routine bodies; PostgreSQL bodies use dollar quoting; SQLite/PostgreSQL
/// `CREATE TRIGGER/FUNCTION/PROCEDURE … BEGIN … END` bodies are kept whole.
pub(crate) fn split(sql: &str, dialect: Dialect) -> Result<Vec<Statement>, ScriptError> {
    if sql.len() > ExecutionLimits::MAX_SCRIPT_BYTES {
        return Err(ScriptError::TooLarge);
    }
    let mut statements = Vec::new();
    split_each(sql, dialect, &mut |statement| {
        statements.push(statement);
        if statements.len() > ExecutionLimits::MAX_STATEMENTS {
            return Err(ScriptError::TooManyStatements);
        }
        Ok(())
    })
    .map_err(|(_, error)| error)?;
    if statements.is_empty() {
        return Err(ScriptError::Empty);
    }
    Ok(statements)
}

/// The splitter itself, handing each statement to `each` as soon as it ends, so a caller can
/// check statements one at a time without holding them all (SQL import). On error it returns
/// the byte offset where the failing statement starts. No size or count limit is applied here.
pub(crate) fn split_each(
    sql: &str,
    dialect: Dialect,
    each: &mut dyn FnMut(Statement) -> Result<(), ScriptError>,
) -> Result<(), (usize, ScriptError)> {
    let mut start = 0;
    scan(sql, dialect, each, &mut start).map_err(|error| (start, error))
}
fn emit(
    each: &mut dyn FnMut(Statement) -> Result<(), ScriptError>,
    sql: &str,
    range: std::ops::Range<usize>,
    words: &mut Vec<String>,
    body: &mut Body,
) -> Result<(), ScriptError> {
    let raw = &sql[range.clone()];
    let text = raw.trim();
    let begin = range.start + (raw.len() - raw.trim_start().len());
    let result = if words.is_empty() {
        Ok(())
    } else {
        each(Statement {
            sql: text.to_owned(),
            span: begin..begin + text.len(),
            words: std::mem::take(words),
            compound: body.compound,
            semicolon: body.semicolon,
            label: body.label,
        })
    };
    words.clear();
    *body = Body::default();
    result
}
fn scan(
    sql: &str,
    dialect: Dialect,
    each: &mut dyn FnMut(Statement) -> Result<(), ScriptError>,
    start: &mut usize,
) -> Result<(), ScriptError> {
    let bytes = sql.as_bytes();
    let mut delimiter: Vec<u8> = b";".to_vec();
    let mut index = 0;
    let mut words: Vec<String> = Vec::new();
    let mut depth = 0usize;
    let mut executable_comment = false;
    // MySQL compound-statement nesting, tracked only to recognise a stray `;` (never to split).
    let mut body = Body::default();
    // SQLite `CREATE TRIGGER` bodies end only at `; END ;` (as in SQLite's sqlite3_complete and
    // its grammar), never at a `BEGIN`/`END` used as an identifier.
    let mut trigger = Trigger::Outside;
    let sqlite = dialect == Dialect::Sqlite;
    while index < bytes.len() {
        let byte = bytes[index];
        // DELIMITER is a client directive, recognized only at the start of a statement.
        if dialect == Dialect::MySql
            && matches!(byte, b'D' | b'd')
            && words.is_empty()
            && sql[*start..index].trim().is_empty()
        {
            let rest = &sql[index..];
            if rest.len() > 10
                && rest[..9].eq_ignore_ascii_case("DELIMITER")
                && rest.as_bytes()[9].is_ascii_whitespace()
            {
                let line_end = rest.find('\n').unwrap_or(rest.len());
                let value = rest[9..line_end].trim();
                if value.is_empty() || value.contains(['\'', '"', '`']) {
                    return Err(ScriptError::Unterminated);
                }
                delimiter = value.as_bytes().to_vec();
                index += line_end;
                *start = index;
                continue;
            }
        }
        if executable_comment && bytes[index..].starts_with(b"*/") {
            executable_comment = false;
            index += 2;
            continue;
        }
        if sqlite && byte == b';' && trigger.consumes_semicolon() {
            index += 1;
            continue;
        }
        if depth == 0 && bytes[index..].starts_with(&delimiter) {
            emit(each, sql, *start..index, &mut words, &mut body)?;
            trigger = Trigger::Outside;
            index += delimiter.len();
            *start = index;
            continue;
        }
        match byte {
            b'\'' | b'"' | b'`' | b'[' => {
                let close = match byte {
                    b'[' if dialect == Dialect::Sqlite => b']',
                    b'[' => {
                        index += 1;
                        continue;
                    }
                    b'`' if dialect == Dialect::Postgres => {
                        index += 1;
                        continue;
                    }
                    other => other,
                };
                let open = index + 1;
                // MySQL strings and PostgreSQL E'' strings honour backslash escapes.
                let escapes = match (dialect, byte) {
                    (Dialect::MySql, b'\'' | b'"') => true,
                    (Dialect::Postgres, b'\'') => {
                        index > 0
                            && matches!(bytes[index - 1], b'E' | b'e')
                            && (index < 2 || !is_word(bytes[index - 2]))
                    }
                    _ => false,
                };
                index = skip_quoted(bytes, open, close, escapes)?;
                trigger.other();
                // Quoted identifiers count as words in SET, so `` SET @@`autocommit` `` is seen.
                if matches!(byte, b'`' | b'"') && words.first().is_some_and(|word| word == "SET") {
                    let quoted = &sql[open..index - 1];
                    if !quoted.is_empty() && quoted.bytes().all(is_word) {
                        words.push(quoted.to_ascii_uppercase());
                    }
                }
            }
            b';' if dialect == Dialect::MySql => {
                body.semicolon();
                index += 1;
            }
            b'-' if bytes.get(index + 1) == Some(&b'-')
                && (dialect != Dialect::MySql
                    || bytes.get(index + 2).is_none_or(|next| {
                        next.is_ascii_whitespace() || next.is_ascii_control()
                    })) =>
            {
                index = sql[index..]
                    .find('\n')
                    .map_or(bytes.len(), |end| index + end + 1);
            }
            b'#' if dialect == Dialect::MySql => {
                index = sql[index..]
                    .find('\n')
                    .map_or(bytes.len(), |end| index + end + 1);
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                // MySQL executable comments run their content: lex it as code.
                if dialect == Dialect::MySql
                    && (bytes.get(index + 2) == Some(&b'!')
                        || bytes[index + 2..].starts_with(b"M!"))
                {
                    index += if bytes[index + 2] == b'!' { 3 } else { 4 };
                    while index < bytes.len() && bytes[index].is_ascii_digit() {
                        index += 1;
                    }
                    executable_comment = true;
                    continue;
                }
                index = skip_block_comment(bytes, index, dialect == Dialect::Postgres)?;
            }
            b'$' if dialect == Dialect::Postgres && (index == 0 || !is_word(bytes[index - 1])) => {
                match dollar_tag(bytes, index) {
                    Some(tag) => {
                        let body = index + tag.len();
                        let end = sql[body..]
                            .find(std::str::from_utf8(tag).unwrap_or("$$"))
                            .ok_or(ScriptError::Unterminated)?;
                        index = body + end + tag.len();
                    }
                    None => index += 1,
                }
            }
            byte if byte.is_ascii_alphabetic() || byte == b'_' => {
                let begin = index;
                while index < bytes.len() && is_word(bytes[index]) {
                    index += 1;
                }
                let word = sql[begin..index].to_ascii_uppercase();
                if dialect == Dialect::MySql {
                    body.word(&word);
                } else if sqlite {
                    trigger.word(&word);
                } else if body_statement(&words) {
                    match word.as_str() {
                        "BEGIN" | "CASE" => depth += 1,
                        "END" => depth = depth.saturating_sub(1),
                        _ => {}
                    }
                }
                // Every word is kept (bounded by MAX_SCRIPT_BYTES) so policy checks see the whole
                // statement, not a leading window.
                words.push(word);
                if sqlite && starts_trigger(&words) {
                    trigger = Trigger::Body;
                }
            }
            b':' if dialect == Dialect::MySql => {
                body.colon(words.len(), bytes.get(index + 1) == Some(&b'='));
                index += 1;
            }
            byte => {
                // SQLite's whitespace also includes vertical tab (0x0B).
                if !byte.is_ascii_whitespace() && byte != 0x0B {
                    trigger.other();
                    body.punctuation();
                }
                index += 1;
            }
        }
    }
    if executable_comment || depth > 0 || trigger.open() {
        return Err(ScriptError::Unterminated);
    }
    emit(each, sql, *start..sql.len(), &mut words, &mut body)
}

/// MySQL/MariaDB compound-statement depth: `BEGIN` and `CASE` open, `END` closes except in
/// `END IF/LOOP/WHILE/REPEAT` (whose openers are not counted); `END CASE` closes once.
#[derive(Default)]
struct Body {
    depth: usize,
    pending_end: bool,
    compound: bool,
    semicolon: bool,
    /// Punctuation other than whitespace seen since the statement started.
    punctuated: bool,
    label: bool,
}
impl Body {
    fn close_pending(&mut self) {
        if std::mem::take(&mut self.pending_end) {
            self.depth = self.depth.saturating_sub(1);
        }
    }
    fn word(&mut self, word: &str) {
        if std::mem::take(&mut self.pending_end) {
            match word {
                "IF" | "LOOP" | "WHILE" | "REPEAT" => return,
                "CASE" => {
                    self.depth = self.depth.saturating_sub(1);
                    return;
                }
                _ => self.depth = self.depth.saturating_sub(1),
            }
        }
        match word {
            "BEGIN" | "CASE" => self.depth += 1,
            "END" => self.pending_end = true,
            _ => {}
        }
    }
    /// `name:` at the start of a statement is a label; `:=` is an assignment.
    fn colon(&mut self, words: usize, assignment: bool) {
        if words == 1 && !self.punctuated && !assignment {
            self.label = true;
        }
        self.punctuated = true;
    }
    fn punctuation(&mut self) {
        self.punctuated = true;
    }
    fn semicolon(&mut self) {
        self.semicolon = true;
        self.punctuated = true;
        self.close_pending();
        if self.depth == 0 {
            self.compound = true;
        }
    }
}

/// SQLite trigger-body tracking, following SQLite's grammar (`BEGIN trigger_cmd ; … END`):
/// inside a trigger a `;` ends the statement only when it follows `; END`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Trigger {
    Outside,
    Body,
    /// Just after a `;` inside the body.
    Semi,
    /// After `; END`: the next `;` ends the statement.
    End,
}
impl Trigger {
    fn consumes_semicolon(&mut self) -> bool {
        if matches!(self, Self::Body | Self::Semi) {
            *self = Self::Semi;
            return true;
        }
        false
    }
    fn word(&mut self, word: &str) {
        match self {
            Self::Outside => {}
            Self::Semi if word == "END" => *self = Self::End,
            _ => *self = Self::Body,
        }
    }
    fn other(&mut self) {
        if *self != Self::Outside {
            *self = Self::Body;
        }
    }
    fn open(self) -> bool {
        matches!(self, Self::Body | Self::Semi)
    }
}
fn starts_trigger(words: &[String]) -> bool {
    match words {
        [create, trigger] => create == "CREATE" && trigger == "TRIGGER",
        [create, temp, trigger] => {
            create == "CREATE"
                && matches!(temp.as_str(), "TEMP" | "TEMPORARY")
                && trigger == "TRIGGER"
        }
        _ => false,
    }
}

fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}
/// Statements whose BEGIN … END bodies contain inner delimiters (SQLite triggers, SQL-standard
/// PostgreSQL routine bodies).
fn body_statement(words: &[String]) -> bool {
    words.first().is_some_and(|word| word == "CREATE")
        && words
            .iter()
            .take(6)
            .any(|word| matches!(word.as_str(), "TRIGGER" | "FUNCTION" | "PROCEDURE"))
}
fn skip_quoted(
    bytes: &[u8],
    mut index: usize,
    close: u8,
    escapes: bool,
) -> Result<usize, ScriptError> {
    while index < bytes.len() {
        if escapes && bytes[index] == b'\\' {
            index += 2;
            continue;
        }
        if bytes[index] == close {
            if close != b']' && bytes.get(index + 1) == Some(&close) {
                index += 2;
                continue;
            }
            return Ok(index + 1);
        }
        index += 1;
    }
    Err(ScriptError::Unterminated)
}
fn skip_block_comment(bytes: &[u8], mut index: usize, nested: bool) -> Result<usize, ScriptError> {
    let mut depth = 0usize;
    while index + 1 < bytes.len() {
        if bytes[index..].starts_with(b"/*") && (nested || depth == 0) {
            depth += 1;
            index += 2;
        } else if bytes[index..].starts_with(b"*/") {
            depth -= 1;
            index += 2;
            if depth == 0 {
                return Ok(index);
            }
        } else {
            index += 1;
        }
    }
    Err(ScriptError::Unterminated)
}
fn dollar_tag(bytes: &[u8], index: usize) -> Option<&[u8]> {
    let mut end = index + 1;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    let tag_ok = end == index + 1 || !bytes[index + 1].is_ascii_digit();
    (bytes.get(end) == Some(&b'$') && tag_ok).then(|| &bytes[index..=end])
}

/// Refuses the whole script before anything runs if any statement violates the policy.
pub(crate) fn check(
    statements: &[Statement],
    dialect: Dialect,
    in_transaction: bool,
) -> Result<(), (usize, ScriptError)> {
    for (position, statement) in statements.iter().enumerate() {
        classify(statement, dialect, in_transaction).map_err(|error| (position, error))?;
    }
    Ok(())
}
/// One statement's policy check (SQL import checks statements as the splitter yields them).
pub(crate) fn check_one(
    statement: &Statement,
    dialect: Dialect,
    in_transaction: bool,
) -> Result<(), ScriptError> {
    classify(statement, dialect, in_transaction)
}
fn classify(
    statement: &Statement,
    dialect: Dialect,
    in_transaction: bool,
) -> Result<(), ScriptError> {
    let words = &statement.words;
    let word = |index: usize| words.get(index).map_or("", String::as_str);
    let first = word(0);
    let control = matches!(
        first,
        "BEGIN" | "COMMIT" | "ROLLBACK" | "XA" | "ABORT" | "SAVEPOINT" | "RELEASE"
    ) || first == "START" && word(1) == "TRANSACTION"
        || first == "END" && dialect != Dialect::MySql
        || first == "PREPARE" && word(1) == "TRANSACTION"
        || first == "SET" && words.iter().any(|word| word == "AUTOCOMMIT");
    if control {
        return Err(ScriptError::TransactionControl);
    }
    if dialect == Dialect::MySql {
        // Checked after transaction control so `SET autocommit` keeps its specific refusal.
        // Wrappers whose inner statement the first-word policy would not see (R-2). Refused in
        // and out of a lease, so PREPARE/EXECUTE stay refused when wrapped too.
        if first == "SET" && word(1) == "STATEMENT" {
            return Err(ScriptError::SetStatement);
        }
        if statement.label || matches!(first, "CASE" | "IF" | "LOOP" | "WHILE" | "REPEAT" | "FOR") {
            return Err(ScriptError::CompoundStatement);
        }
        // A procedure can SET autocommit, START TRANSACTION, COMMIT or PREPARE internally (R-3).
        if first == "CALL" {
            return Err(ScriptError::Call);
        }
        if statement.compound {
            return Err(ScriptError::MultipleStatements);
        }
        if matches!(first, "PREPARE" | "EXECUTE" | "DEALLOCATE")
            || first == "DROP" && word(1) == "PREPARE"
        {
            return Err(ScriptError::PreparedStatement);
        }
        if in_transaction && !allowed_in_transaction(statement, first, word(1)) {
            return Err(ScriptError::NotAllowedInTransaction);
        }
    }
    if dialect == Dialect::Sqlite
        && (matches!(first, "ATTACH" | "DETACH")
            || first == "VACUUM" && words.iter().any(|word| word == "INTO"))
    {
        return Err(ScriptError::FileAccess);
    }
    Ok(())
}

/// The statement kinds accepted inside a MySQL/MariaDB lease (an allowlist, not a denylist of
/// implicit commits). None of them commits implicitly or changes autocommit, and a stored
/// function or trigger they invoke cannot COMMIT (the server refuses it, ER 1422):
/// - SELECT, WITH, VALUES, TABLE, SHOW, DESCRIBE/DESC, EXPLAIN: reads (EXPLAIN ANALYZE runs only
///   statements that are themselves allowed);
/// - INSERT, UPDATE, DELETE, REPLACE: transactional row changes, the purpose of a lease;
/// - USE: changes only the session's default schema;
/// - CREATE/DROP TEMPORARY TABLE: documented as not committing implicitly;
/// - SET of user variables (`@name`) only: no system variable (autocommit, completion_type,
///   transaction characteristics, sql_mode …) can be changed inside a lease.
///
/// The statement must not contain `;` anywhere, so nothing can follow it on the text protocol.
fn allowed_in_transaction(statement: &Statement, first: &str, second: &str) -> bool {
    let kind = match first {
        "SELECT" | "WITH" | "VALUES" | "TABLE" | "SHOW" | "DESCRIBE" | "DESC" | "EXPLAIN"
        | "INSERT" | "UPDATE" | "DELETE" | "REPLACE" | "USE" => true,
        "CREATE" | "DROP" => second == "TEMPORARY",
        "SET" => sets_only_user_variables(&statement.sql),
        _ => false,
    };
    kind && !statement.semicolon
}
/// `SET @a = …, @b := …`: every top-level assignment target is a user variable (one `@`).
/// Conservative: any comment (including executable comments) refuses the statement.
fn sets_only_user_variables(sql: &str) -> bool {
    if sql.contains("/*") || sql.contains('#') || sql.contains("--") {
        return false;
    }
    let text = sql.trim_start();
    if text.len() < 4 || !text[..3].eq_ignore_ascii_case("SET") {
        return false;
    }
    let bytes = &text.as_bytes()[3..];
    if !bytes.first().is_some_and(u8::is_ascii_whitespace) {
        return false;
    }
    let mut index = 0;
    loop {
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        if bytes.get(index) != Some(&b'@') || matches!(bytes.get(index + 1), Some(b'@') | None) {
            return false;
        }
        // Skip to the next top-level comma (outside quotes and parentheses).
        let mut depth = 0usize;
        loop {
            match bytes.get(index) {
                None => return true,
                Some(b'\'' | b'"' | b'`') => {
                    let close = bytes[index];
                    match skip_quoted(bytes, index + 1, close, close != b'`') {
                        Ok(next) => index = next,
                        Err(_) => return false,
                    }
                    continue;
                }
                Some(b'(') => depth += 1,
                Some(b')') => depth = depth.saturating_sub(1),
                Some(b',') if depth == 0 => {
                    index += 1;
                    break;
                }
                _ => {}
            }
            index += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn texts(sql: &str, dialect: Dialect) -> Vec<String> {
        split(sql, dialect)
            .unwrap()
            .into_iter()
            .map(|statement| statement.sql)
            .collect()
    }
    #[test]
    fn splits_outside_quotes_comments_and_bodies_without_rewriting() {
        assert_eq!(
            texts(
                "SELECT ';'; -- a; comment\nINSERT INTO t VALUES ('it''s;', \"x;\", `a;b`); # c;\nSELECT 1",
                Dialect::MySql
            ),
            [
                "SELECT ';'",
                "-- a; comment\nINSERT INTO t VALUES ('it''s;', \"x;\", `a;b`)",
                "# c;\nSELECT 1"
            ]
        );
        assert_eq!(
            texts("SELECT 'a\\';b'; SELECT 2;", Dialect::MySql),
            ["SELECT 'a\\';b'", "SELECT 2"]
        );
        assert_eq!(
            texts(
                "CREATE FUNCTION f() RETURNS int AS $body$ BEGIN RETURN 1; END $body$ LANGUAGE plpgsql; SELECT $$;$$, E'x\\';', 'y\\'; /* a /* nested; */ ; */ SELECT 3",
                Dialect::Postgres
            ),
            [
                "CREATE FUNCTION f() RETURNS int AS $body$ BEGIN RETURN 1; END $body$ LANGUAGE plpgsql",
                "SELECT $$;$$, E'x\\';', 'y\\'",
                "/* a /* nested; */ ; */ SELECT 3"
            ]
        );
        assert_eq!(
            texts(
                "CREATE TRIGGER t AFTER INSERT ON a BEGIN UPDATE b SET n = CASE WHEN 1 THEN 2 END; DELETE FROM c; END; SELECT [x;y] FROM a",
                Dialect::Sqlite
            ),
            [
                "CREATE TRIGGER t AFTER INSERT ON a BEGIN UPDATE b SET n = CASE WHEN 1 THEN 2 END; DELETE FROM c; END",
                "SELECT [x;y] FROM a"
            ]
        );
    }
    #[test]
    fn mysql_delimiter_directive_keeps_routine_bodies_whole() {
        let statements = texts(
            "DELIMITER //\nCREATE PROCEDURE p() BEGIN SELECT 1; SELECT 2; END//\nDELIMITER ;\nCALL p();",
            Dialect::MySql,
        );
        assert_eq!(
            statements,
            [
                "CREATE PROCEDURE p() BEGIN SELECT 1; SELECT 2; END",
                "CALL p()"
            ]
        );
    }
    #[test]
    fn empty_unterminated_and_oversized_scripts_are_refused() {
        assert_eq!(split(" ; -- x\n;", Dialect::MySql), Err(ScriptError::Empty));
        for (sql, dialect) in [
            ("SELECT 'x", Dialect::MySql),
            ("SELECT /* x", Dialect::Postgres),
            ("SELECT $a$ x", Dialect::Postgres),
            ("SELECT /*!40101 1", Dialect::MySql),
            (
                "CREATE TRIGGER t AFTER INSERT ON a BEGIN SELECT 1;",
                Dialect::Sqlite,
            ),
        ] {
            assert_eq!(split(sql, dialect), Err(ScriptError::Unterminated), "{sql}");
        }
        assert_eq!(
            split(
                &"SELECT 1;".repeat(ExecutionLimits::MAX_STATEMENTS + 1),
                Dialect::MySql
            ),
            Err(ScriptError::TooManyStatements)
        );
        assert_eq!(
            split(
                &" ".repeat(ExecutionLimits::MAX_SCRIPT_BYTES + 1),
                Dialect::MySql
            ),
            Err(ScriptError::TooLarge)
        );
    }
    fn policy(sql: &str, dialect: Dialect, lease: bool) -> Result<(), (usize, ScriptError)> {
        check(&split(sql, dialect).unwrap(), dialect, lease)
    }
    #[test]
    fn transaction_control_and_savepoints_are_refused_everywhere() {
        for (sql, dialect) in [
            ("SELECT 1; BEGIN", Dialect::MySql),
            ("START TRANSACTION READ WRITE", Dialect::MySql),
            ("commit", Dialect::Postgres),
            ("ROLLBACK", Dialect::Sqlite),
            ("END", Dialect::Postgres),
            ("SET @@session.autocommit = 1", Dialect::MySql),
            ("/*!40101 SET autocommit=0 */", Dialect::MySql),
            ("XA START 'x'", Dialect::MySql),
            ("PREPARE TRANSACTION 'x'", Dialect::Postgres),
            ("ABORT", Dialect::Postgres),
            ("SAVEPOINT a", Dialect::Postgres),
            ("RELEASE SAVEPOINT a", Dialect::MySql),
            ("ROLLBACK TO SAVEPOINT a", Dialect::Sqlite),
        ] {
            for lease in [false, true] {
                assert_eq!(
                    policy(sql, dialect, lease).unwrap_err().1,
                    ScriptError::TransactionControl,
                    "{sql}"
                );
            }
        }
        assert_eq!(
            policy("SELECT 1; SAVEPOINT a", Dialect::Postgres, true),
            Err((1, ScriptError::TransactionControl))
        );
        let script = "SET @x = 1; SET TRANSACTION ISOLATION LEVEL READ COMMITTED";
        assert!(policy(script, Dialect::MySql, false).is_ok());
        // Inside a lease only user variables may be set.
        assert_eq!(
            policy(script, Dialect::MySql, true),
            Err((1, ScriptError::NotAllowedInTransaction))
        );
    }
    #[test]
    fn mysql_implicit_commits_are_refused_only_inside_a_lease() {
        for sql in [
            "CREATE TABLE t(id int)",
            "ALTER TABLE t ADD c int",
            "TRUNCATE t",
            "LOCK TABLES t WRITE",
            "SET PASSWORD = 'x'",
            "/*!50001 DROP VIEW v */",
        ] {
            assert_eq!(
                policy(sql, Dialect::MySql, true),
                Err((0, ScriptError::NotAllowedInTransaction)),
                "{sql}"
            );
            assert!(policy(sql, Dialect::MySql, false).is_ok(), "{sql}");
        }
        assert!(
            policy(
                "CREATE TEMPORARY TABLE t(id int); INSERT INTO t VALUES (1)",
                Dialect::MySql,
                true
            )
            .is_ok()
        );
        // PostgreSQL DDL is transactional.
        assert!(policy("CREATE TABLE t(id int); CALL p()", Dialect::Postgres, true).is_ok());
    }
    #[test]
    fn autocommit_changes_are_refused_anywhere_in_a_set_statement() {
        for (sql, dialect) in [
            // Beyond any leading-word window (V-2a).
            (
                "SET @v1=0,@v2=0,@v3=0,@v4=0,@v5=0,@v6=0,@v7=0,@v8=0,@v9=0,autocommit=0",
                Dialect::MySql,
            ),
            ("SET GLOBAL autocommit = 0", Dialect::MySql),
            ("SET @@autocommit := 0", Dialect::MySql),
            ("set /*c*/ session AutoCommit = 1", Dialect::MySql),
            ("SET SESSION autocommit=OFF", Dialect::MySql),
            ("SET @@session.`autocommit` = 0", Dialect::MySql),
            ("SET @@SESSION.\"AUTOCOMMIT\" = 0", Dialect::MySql),
            ("SET @x = 1, -- note\n autocommit = 0", Dialect::MySql),
            ("SET @x = 1, # note\n autocommit = 0", Dialect::MySql),
            (
                "/*!40101 SET @a=1, @b=2, @c=3, @d=4, @e=5, @f=6, @g=7, @h=8, autocommit=0 */",
                Dialect::MySql,
            ),
            (
                "SET STATEMENT autocommit=0 FOR INSERT INTO t VALUES (1)",
                Dialect::MySql,
            ),
            ("SET autocommit TO off", Dialect::Postgres),
        ] {
            for lease in [false, true] {
                assert_eq!(
                    policy(sql, dialect, lease),
                    Err((0, ScriptError::TransactionControl)),
                    "{sql}"
                );
            }
        }
        // A string value that merely mentions the word is data, not a variable.
        assert!(policy("SET @x = 'autocommit'", Dialect::MySql, true).is_ok());
    }
    #[test]
    fn mysql_prepared_statements_are_refused_in_and_out_of_a_lease() {
        for sql in [
            "PREPARE s FROM 'COMMIT'",
            "EXECUTE s",
            "EXECUTE IMMEDIATE 'COMMIT'",
            "DEALLOCATE PREPARE s",
            "DROP PREPARE s",
            "prepare s from @sql",
            "/*!50000 PREPARE s FROM 'SET autocommit = 0' */",
        ] {
            for lease in [false, true] {
                assert_eq!(
                    policy(sql, Dialect::MySql, lease),
                    Err((0, ScriptError::PreparedStatement)),
                    "{sql}"
                );
            }
        }
        // V-2b: the first refused statement is reported; nothing in the script runs.
        assert_eq!(
            policy(
                "SELECT 1; PREPARE s FROM 'COMMIT'; EXECUTE s",
                Dialect::MySql,
                true
            ),
            Err((1, ScriptError::PreparedStatement))
        );
        // PostgreSQL prepared statements cannot contain transaction control.
        assert!(policy("PREPARE p AS SELECT 1; EXECUTE p", Dialect::Postgres, true).is_ok());
    }
    #[test]
    fn mysql_custom_delimiters_cannot_smuggle_a_second_statement() {
        for sql in [
            "DELIMITER //\nLOCK TABLES t WRITE; START TRANSACTION//",
            "DELIMITER //\nCREATE PROCEDURE p() SELECT 1; COMMIT//",
            "DELIMITER //\nCREATE PROCEDURE p() BEGIN SELECT 1; END; COMMIT//",
            "DELIMITER //\nSELECT 1; SET @x = 1//",
            "DELIMITER $$\nCREATE TRIGGER t BEFORE INSERT ON a FOR EACH ROW BEGIN IF NEW.x THEN SET NEW.y = 1; END IF; END; DROP TABLE a$$",
        ] {
            assert_eq!(
                policy(sql, Dialect::MySql, false),
                Err((0, ScriptError::MultipleStatements)),
                "{sql}"
            );
        }
        let routine = "DELIMITER //\nCREATE PROCEDURE p() BEGIN DECLARE x INT DEFAULT 1; IF x THEN SELECT 1; END IF; CASE x WHEN 1 THEN SELECT 2; END CASE; l: LOOP LEAVE l; END LOOP l; WHILE x > 1 DO SET x = 0; END WHILE; SELECT CASE WHEN x THEN 1 END; COMMIT; END//\nDELIMITER ;\nSELECT 1;";
        assert!(policy(routine, Dialect::MySql, false).is_ok());
        assert_eq!(
            policy(routine, Dialect::MySql, true),
            Err((0, ScriptError::NotAllowedInTransaction))
        );
    }
    #[test]
    fn other_transaction_and_implicit_commit_vectors_stay_refused() {
        for sql in [
            "BEGIN NOT ATOMIC SELECT 1; END",
            "BEGIN WORK",
            "COMMIT AND CHAIN",
            "ROLLBACK AND NO CHAIN RELEASE",
            "XA END 'x'",
            "START TRANSACTION WITH CONSISTENT SNAPSHOT, READ ONLY",
            "/*!40101 START TRANSACTION */",
        ] {
            for lease in [false, true] {
                assert_eq!(
                    policy(sql, Dialect::MySql, lease).unwrap_err().1,
                    ScriptError::TransactionControl,
                    "{sql}"
                );
            }
        }
        for sql in [
            "LOCK TABLES t WRITE",
            "LOCK INSTANCE FOR BACKUP",
            "UNLOCK TABLES",
            "RENAME TABLE a TO b",
            "CREATE INDEX i ON t(a)",
            "DROP TABLE t",
            "GRANT SELECT ON t TO u",
            "BACKUP STAGE START",
            "LOAD INDEX INTO CACHE t",
            // Not implicit commits, but outside the in-lease allowlist.
            "SET TRANSACTION READ WRITE",
            "SET SESSION sql_mode = ''",
            "HANDLER t OPEN",
            "DO 1",
            "ANALYZE SELECT 1",
        ] {
            assert_eq!(
                policy(sql, Dialect::MySql, true),
                Err((0, ScriptError::NotAllowedInTransaction)),
                "{sql}"
            );
        }
        // SET TRANSACTION changes only the next transaction's characteristics; outside a lease
        // it is not an autocommit or commit vector.
        assert!(policy("SET TRANSACTION READ WRITE", Dialect::MySql, false).is_ok());
    }
    #[test]
    fn sqlite_cannot_reach_other_files() {
        for sql in [
            "ATTACH 'x.db' AS x",
            "DETACH x",
            "VACUUM INTO '/tmp/copy.db'",
        ] {
            assert_eq!(
                policy(sql, Dialect::Sqlite, false),
                Err((0, ScriptError::FileAccess))
            );
        }
        assert!(policy("VACUUM", Dialect::Sqlite, false).is_ok());
    }

    #[test]
    fn r2_mariadb_wrappers_and_compound_statements_are_refused_in_and_out_of_a_lease() {
        for (sql, error) in [
            (
                "SET STATEMENT max_statement_time=60 FOR CREATE TABLE z(i int)",
                ScriptError::SetStatement,
            ),
            (
                "SET STATEMENT max_statement_time=60 FOR COMMIT",
                ScriptError::SetStatement,
            ),
            ("set statement x=1 for CALL p()", ScriptError::SetStatement),
            (
                "SET STATEMENT x=1 FOR TRUNCATE t",
                ScriptError::SetStatement,
            ),
            (
                "SET STATEMENT x=1 FOR LOCK TABLES t WRITE",
                ScriptError::SetStatement,
            ),
            (
                "SET STATEMENT x=1 FOR PREPARE s FROM 'COMMIT'",
                ScriptError::SetStatement,
            ),
            ("SET STATEMENT x=1 FOR EXECUTE s", ScriptError::SetStatement),
            (
                "/*M!100301 SET STATEMENT x=1 FOR COMMIT */",
                ScriptError::SetStatement,
            ),
            (
                "SET STATEMENT x=1 FOR INSERT INTO t VALUES (1)",
                ScriptError::SetStatement,
            ),
            (
                "DELIMITER //\nCASE WHEN 1 THEN COMMIT; END CASE//",
                ScriptError::CompoundStatement,
            ),
            (
                "DELIMITER //\nCASE WHEN 1 THEN SET @x = 1; END CASE//",
                ScriptError::CompoundStatement,
            ),
            (
                "CASE WHEN 1 THEN SELECT 1 END CASE",
                ScriptError::CompoundStatement,
            ),
            (
                "DELIMITER //\nIF 1 THEN COMMIT; END IF//",
                ScriptError::CompoundStatement,
            ),
            (
                "DELIMITER //\nWHILE 0 DO COMMIT; END WHILE//",
                ScriptError::CompoundStatement,
            ),
            (
                "DELIMITER //\nREPEAT COMMIT; UNTIL 1 END REPEAT//",
                ScriptError::CompoundStatement,
            ),
            (
                "DELIMITER //\nLOOP COMMIT; LEAVE; END LOOP//",
                ScriptError::CompoundStatement,
            ),
            (
                "DELIMITER //\nFOR i IN 1..2 DO COMMIT; END FOR//",
                ScriptError::CompoundStatement,
            ),
            (
                "DELIMITER //\nl1: BEGIN NOT ATOMIC COMMIT; END//",
                ScriptError::CompoundStatement,
            ),
            (
                "DELIMITER //\nl1 : LOOP COMMIT; END LOOP//",
                ScriptError::CompoundStatement,
            ),
            ("CALL p()", ScriptError::Call),
            ("/*!50000 CALL p() */", ScriptError::Call),
            ("call exec_autocommit_off()", ScriptError::Call),
        ] {
            for lease in [false, true] {
                assert_eq!(policy(sql, Dialect::MySql, lease), Err((0, error)), "{sql}");
            }
        }
        // `SET STATEMENT autocommit=…` keeps its specific refusal.
        assert_eq!(
            policy(
                "SET STATEMENT autocommit=0 FOR SELECT 1",
                Dialect::MySql,
                true
            ),
            Err((0, ScriptError::TransactionControl))
        );
        // `:=` is an assignment, not a label; CASE inside an expression is not a statement.
        for sql in [
            "SET @x := 1",
            "SELECT @x := 1",
            "SELECT CASE WHEN 1 THEN 2 END",
            "UPDATE t SET a = CASE WHEN b THEN 1 ELSE 2 END",
        ] {
            for lease in [false, true] {
                assert!(policy(sql, Dialect::MySql, lease).is_ok(), "{sql}");
            }
        }
        // PostgreSQL CASE/IF/CALL keep the server's semantics (no MySQL wrappers there).
        assert!(policy("CALL p()", Dialect::Postgres, true).is_ok());
    }
    #[test]
    fn r2_inside_a_mysql_lease_only_allowlisted_statements_run() {
        for sql in [
            "SELECT 1",
            "(SELECT 1) UNION (SELECT 2)",
            "WITH c AS (SELECT 1) SELECT * FROM c",
            "INSERT INTO t VALUES (1)",
            "UPDATE t SET a = 1",
            "DELETE FROM t",
            "REPLACE INTO t VALUES (1)",
            "VALUES ROW(1)",
            "TABLE t",
            "SHOW TABLES",
            "DESCRIBE t",
            "DESC t",
            "EXPLAIN SELECT 1",
            "USE other",
            "CREATE TEMPORARY TABLE t(id int)",
            "DROP TEMPORARY TABLE t",
            "SET @a = 1",
            "SET @a = 1, @b := (SELECT COUNT(*) FROM t WHERE x IN (1, 2)), @`c d` = 'x,@@y'",
            "set\t@'quoted' = \"a,b\"",
        ] {
            assert!(policy(sql, Dialect::MySql, true).is_ok(), "{sql}");
        }
        for sql in [
            "SET @a = 1, autocommit_like = 0",
            "SET @a = 1, sql_mode = ''",
            "SET @a = 1, @@session.sql_mode = ''",
            "SET @@completion_type = 'CHAIN'",
            "SET completion_type = 2",
            "SET NAMES latin1",
            "SET SESSION TRANSACTION ISOLATION LEVEL SERIALIZABLE",
            "SET @a = 1 /* c */",
            "/*!40101 SET @a = 1 */",
            "SET ROLE ALL",
            "ALTER TEMPORARY TABLE t ADD c int",
            "LOAD DATA LOCAL INFILE 'x' INTO TABLE t",
            "HELP 'x'",
            "BINLOG 'x'",
            "KILL 1",
            // A `;` anywhere (custom DELIMITER) is refused inside a lease, even in a body.
            "DELIMITER //\nSELECT 1 AS begin; SELECT 2//",
            "DELIMITER //\nINSERT INTO t SELECT 1 AS `x;y`; COMMIT//",
        ] {
            assert!(policy(sql, Dialect::MySql, true).is_err(), "{sql}");
        }
        assert_eq!(
            policy(
                "DELIMITER //\nSELECT 1 AS begin; COMMIT//",
                Dialect::MySql,
                true
            ),
            Err((0, ScriptError::NotAllowedInTransaction))
        );
    }
    #[test]
    fn r1_sqlite_trigger_bodies_end_only_at_semicolon_end_semicolon() {
        // The review's smuggling input: `begin`/`end` used as identifiers must not move the
        // trigger boundary, so the COMMIT and BEGIN are separate (refused) statements.
        let smuggle = "CREATE TEMP TRIGGER rv AFTER INSERT ON parent BEGIN SELECT 1 AS begin; END; COMMIT; BEGIN; SELECT 1 AS end, 2 AS end";
        assert_eq!(
            texts(smuggle, Dialect::Sqlite),
            [
                "CREATE TEMP TRIGGER rv AFTER INSERT ON parent BEGIN SELECT 1 AS begin; END",
                "COMMIT",
                "BEGIN",
                "SELECT 1 AS end, 2 AS end"
            ]
        );
        for lease in [false, true] {
            assert_eq!(
                policy(smuggle, Dialect::Sqlite, lease),
                Err((1, ScriptError::TransactionControl))
            );
        }
        assert_eq!(
            texts(
                "CREATE TRIGGER t AFTER INSERT ON a BEGIN SELECT CASE WHEN 1 THEN 2 END; SELECT 1 AS end; END; SELECT 3",
                Dialect::Sqlite
            ),
            [
                "CREATE TRIGGER t AFTER INSERT ON a BEGIN SELECT CASE WHEN 1 THEN 2 END; SELECT 1 AS end; END",
                "SELECT 3"
            ]
        );
        assert_eq!(
            split(
                "CREATE TRIGGER t AFTER INSERT ON a BEGIN SELECT 1 AS end;",
                Dialect::Sqlite
            ),
            Err(ScriptError::Unterminated)
        );
    }
}
