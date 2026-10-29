use serde_json::{Value, json};
use stoneql_contracts::v1::*;
use stoneql_core::{CoreError, ErrorCode, Outcome};

fn request() -> Value {
    json!({"version":1,"requestId":"request_1","payload":{"connectionId":"connection_1","database":null,"sql":"SELECT 'raw'; -- unchanged","maxRows":200,"maxBytes":262144}})
}
#[test]
fn accepts_sql_without_interpreting_or_modifying_it() {
    let original = request();
    let decoded = decode_query_request(&original.to_string()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), original);
}
#[test]
fn versions_and_malformed_envelopes_fail_closed() {
    for version in [json!(0), json!(2), json!("1"), json!(null)] {
        let mut value = request();
        value["version"] = version;
        assert_eq!(
            decode_query_request(&value.to_string()).unwrap_err().code,
            ErrorCode::UnsupportedVersion
        );
    }
    assert!(
        serde_json::from_value::<Envelope<QueryRequest>>(
            json!({"version":2,"requestId":"r","payload":request()["payload"]})
        )
        .is_err()
    );
    let duplicate = r#"{"version":2,"version":1,"requestId":"r","payload":{"connectionId":"c","database":null,"sql":"SELECT 1","maxRows":1,"maxBytes":1}}"#;
    assert!(decode_query_request(duplicate).is_err());
    for input in ["not json", "[]", "null", "{}"] {
        assert!(decode_query_request(input).is_err());
    }
    let mut value = request();
    value["secret"] = json!("do not accept");
    assert!(decode_query_request(&value.to_string()).is_err());
    let mut value = request();
    value["payload"]["password"] = json!("do not accept");
    assert!(decode_query_request(&value.to_string()).is_err());
}
#[test]
fn limits_and_opaque_ids_are_validated() {
    for value in [0, 1001] {
        let mut r = request();
        r["payload"]["maxRows"] = json!(value);
        assert!(decode_query_request(&r.to_string()).is_err());
    }
    for value in [0, 1048577] {
        let mut r = request();
        r["payload"]["maxBytes"] = json!(value);
        assert!(decode_query_request(&r.to_string()).is_err());
    }
    for id in ["", "../other", "request with spaces"] {
        let mut r = request();
        r["requestId"] = json!(id);
        assert!(decode_query_request(&r.to_string()).is_err());
    }
    let mut r = request();
    r["payload"]["sql"] = json!("x".repeat(65537));
    assert!(decode_query_request(&r.to_string()).is_err());
}
#[test]
fn raw_values_match_golden_json_without_precision_or_timezone_conversion() {
    let fixture = include_str!("fixtures/values.json");
    let cells: Vec<CellValue> = serde_json::from_str(fixture).unwrap();
    assert_eq!(
        serde_json::to_value(&cells).unwrap(),
        serde_json::from_str::<Value>(fixture).unwrap()
    );
    assert_ne!(cells[0], cells[1]);
    assert!(
        serde_json::from_str::<CellValue>(r#"{"kind":"integer","text":9007199254740993}"#).is_err()
    );
}
#[test]
fn null_default_empty_and_expression_remain_distinct() {
    let values = vec![
        InputValue::Null,
        InputValue::Default,
        InputValue::Value {
            value: CellValue::Text {
                text: String::new(),
            },
        },
        InputValue::Expression {
            sql: "CURRENT_TIMESTAMP".into(),
        },
    ];
    let json = serde_json::to_string(&values).unwrap();
    assert_eq!(
        serde_json::from_str::<Vec<InputValue>>(&json).unwrap(),
        values
    );
    for pair in values.windows(2) {
        assert_ne!(pair[0], pair[1]);
    }
}
#[test]
fn multiple_result_sets_and_structured_errors_round_trip() {
    let result = Envelope {
        version: ProtocolVersion,
        request_id: "r".into(),
        payload: QueryResult {
            result_sets: vec![
                ResultSet {
                    index: 0,
                    columns: vec![Column {
                        name: "x".into(),
                        database_type: "BIGINT".into(),
                        nullable: None,
                    }],
                    rows: vec![vec![CellValue::Integer {
                        text: "9007199254740993".into(),
                    }]],
                    affected_rows: "0".into(),
                    truncated: false,
                },
                ResultSet {
                    index: 1,
                    columns: vec![],
                    rows: vec![],
                    affected_rows: "18446744073709551615".into(),
                    truncated: false,
                },
            ],
        },
    };
    assert_eq!(
        serde_json::from_str::<Envelope<QueryResult>>(&serde_json::to_string(&result).unwrap())
            .unwrap(),
        result
    );
    let error = ErrorDto::from(CoreError::connection_lost());
    assert_eq!(error.outcome, OutcomeDto::Unknown);
    assert!(!error.retryable);
    assert_eq!(
        serde_json::from_str::<ErrorDto>(&serde_json::to_string(&error).unwrap()).unwrap(),
        error
    );
    for outcome in [
        Outcome::Rejected,
        Outcome::RolledBack,
        Outcome::Committed,
        Outcome::Unknown,
    ] {
        let error = ErrorDto::from(CoreError {
            code: ErrorCode::Internal,
            outcome,
        });
        assert!(!error.retryable);
        assert!(!format!("{error:?}").contains("password"));
    }
}
#[test]
fn generated_contract_uses_literal_version_and_string_values() {
    let types = typescript();
    assert!(types.contains("version: 1"));
    assert!(!types.contains("bigint"));
    assert_eq!(types, typescript());
}

#[test]
fn engine_identifiers_are_stable_machine_names() {
    let engines = [
        EngineDto::MySql,
        EngineDto::MariaDb,
        EngineDto::PostgreSql,
        EngineDto::Sqlite,
    ];
    assert_eq!(
        serde_json::to_value(engines).unwrap(),
        json!(["mysql", "mariadb", "postgresql", "sqlite"])
    );
}
#[test]
fn live_query_limits_are_optional_but_never_null() {
    let omitted = json!({"version":1,"requestId":"read","payload":{"sessionId":"s","handle":"h","sql":"SELECT 1"}});
    let decoded: Envelope<LiveQueryRequest> = decode_envelope(&omitted.to_string()).unwrap();
    assert_eq!(
        (decoded.payload.max_rows, decoded.payload.max_bytes),
        (None, None)
    );
    assert_eq!(serde_json::to_value(decoded).unwrap(), omitted);
    let mut explicit = omitted.clone();
    explicit["payload"]["maxRows"] = json!(10);
    explicit["payload"]["maxBytes"] = json!(1024);
    let decoded: Envelope<LiveQueryRequest> = decode_envelope(&explicit.to_string()).unwrap();
    assert_eq!(
        (decoded.payload.max_rows, decoded.payload.max_bytes),
        (Some(10), Some(1024))
    );
    for field in ["maxRows", "maxBytes"] {
        let mut null = omitted.clone();
        null["payload"][field] = json!(null);
        assert!(decode_envelope::<LiveQueryRequest>(&null.to_string()).is_err());
    }
}
#[test]
fn sqlite_requests_carry_grants_never_paths() {
    let connect = json!({"version":1,"requestId":"open","payload":{"sessionId":"s","grant":"g"}});
    let decoded: Envelope<SqliteConnectionRequest> = decode_envelope(&connect.to_string()).unwrap();
    assert_eq!(decoded.payload.grant, "g");
    let mut path = connect.clone();
    path["payload"]["path"] = json!("/etc/passwd");
    assert!(decode_envelope::<SqliteConnectionRequest>(&path.to_string()).is_err());
    let pick =
        json!({"version":1,"requestId":"pick","payload":{"sessionId":"s","path":"/tmp/x.db"}});
    assert!(decode_envelope::<SqliteFileRequest>(&pick.to_string()).is_err());
    let cancelled = serde_json::to_value(SqliteFileGrant {
        grant: None,
        file_name: None,
    })
    .unwrap();
    assert_eq!(cancelled, json!({"grant":null,"fileName":null}));
}
#[test]
fn ssh_requests_are_strict_and_carry_references_or_grants_never_key_material() {
    let direct = json!({"version":1,"requestId":"open","payload":{"sessionId":"s","engine":"mysql","host":"db.internal","port":3306,"username":"reader","password":"","database":"app","rootCertificatePem":null}});
    let decoded: Envelope<DirectConnectionRequest> = decode_envelope(&direct.to_string()).unwrap();
    assert!(decoded.payload.ssh.is_none() && decoded.payload.password_ref.is_none());
    assert_eq!(serde_json::to_value(decoded).unwrap(), direct);
    let mut tunnel = direct.clone();
    tunnel["payload"]["passwordRef"] = json!("00000000-0000-4000-8000-000000000001");
    tunnel["payload"]["attemptId"] = json!("attempt_1");
    tunnel["payload"]["ssh"] = json!({"host":"bastion","port":22,"username":"deploy","hostKey":"ssh-ed25519 AAAA","auth":{"method":"key_file","grant":"g","passphraseRef":"r"}});
    let decoded: Envelope<DirectConnectionRequest> = decode_envelope(&tunnel.to_string()).unwrap();
    assert!(matches!(
        decoded.payload.ssh.as_ref().unwrap().auth,
        SshAuthRequest::KeyFile { ref passphrase_ref, passphrase: None, .. } if passphrase_ref.as_deref() == Some("r")
    ));
    assert_eq!(serde_json::to_value(decoded).unwrap(), tunnel);
    for (path, value) in [
        (
            "/payload/ssh/auth/privateKey",
            json!("-----BEGIN OPENSSH PRIVATE KEY-----"),
        ),
        ("/payload/ssh/auth/path", json!("/Users/me/.ssh/id_ed25519")),
        ("/payload/ssh/hostKey", json!(null)),
        ("/payload/ssh/auth/passphrase", json!(null)),
        ("/payload/ssh/forwardAgent", json!(true)),
    ] {
        let mut invalid = tunnel.clone();
        let (parent, field) = path.rsplit_once('/').unwrap();
        invalid.pointer_mut(parent).unwrap()[field] = value;
        assert!(
            decode_envelope::<DirectConnectionRequest>(&invalid.to_string()).is_err(),
            "{path}"
        );
    }
    let agent = json!({"method":"agent"});
    assert!(matches!(
        serde_json::from_value::<SshAuthRequest>(agent).unwrap(),
        SshAuthRequest::Agent
    ));
    let store = json!({"version":1,"requestId":"store","payload":{"sessionId":"s","purpose":"ssh_passphrase","target":{"engine":"postgresql","host":"db","port":5432,"username":"u","ssh":{"host":"b","port":22,"username":"d"}},"secret":"x"}});
    let decoded: Envelope<StoreSecretRequest> = decode_envelope(&store.to_string()).unwrap();
    assert_eq!(decoded.payload.purpose, SecretPurpose::SshPassphrase);
    assert_eq!(
        serde_json::to_value(SecretReference {
            reference: "r".into()
        })
        .unwrap(),
        json!({"reference":"r"})
    );
    let error: ErrorDto = CoreError::rejected(ErrorCode::SshHostKeyChanged).into();
    assert_eq!(
        serde_json::to_value(&error).unwrap()["code"],
        json!("SSH_HOST_KEY_CHANGED")
    );
}
#[test]
fn execution_requests_decode_strictly_and_omit_absent_options() {
    let submit = json!({"version":1,"requestId":"r","payload":{"sessionId":"s","handle":"h","sql":"UPDATE t SET a = 1; SELECT 1"}});
    let decoded: Envelope<SubmitJobRequest> = decode_envelope(&submit.to_string()).unwrap();
    assert_eq!(serde_json::to_value(&decoded).unwrap(), submit);
    let mut with_options = submit.clone();
    with_options["payload"]["leaseId"] = json!("l");
    with_options["payload"]["maxRowsPerResult"] = json!(10);
    with_options["payload"]["deadlineSeconds"] = json!(5);
    assert!(decode_envelope::<SubmitJobRequest>(&with_options.to_string()).is_ok());
    for (field, value) in [
        ("leaseId", json!(null)),
        ("maxRowsPerResult", json!(-1)),
        ("retry", json!(true)),
    ] {
        let mut invalid = submit.clone();
        invalid["payload"][field] = value;
        assert!(
            decode_envelope::<SubmitJobRequest>(&invalid.to_string()).is_err(),
            "{field}"
        );
    }
    let end = json!({"version":1,"requestId":"r","payload":{"sessionId":"s","leaseId":"l","action":"commit"}});
    assert!(decode_envelope::<EndTransactionRequest>(&end.to_string()).is_ok());
    let mut savepoint = end.clone();
    savepoint["payload"]["action"] = json!("savepoint");
    assert!(decode_envelope::<EndTransactionRequest>(&savepoint.to_string()).is_err());
    let read_write = json!({"version":1,"requestId":"r","payload":{"sessionId":"s","grant":"g","access":"read_write"}});
    assert!(decode_envelope::<SqliteConnectionRequest>(&read_write.to_string()).is_ok());
    let mut admin = read_write.clone();
    admin["payload"]["access"] = json!("admin");
    assert!(decode_envelope::<SqliteConnectionRequest>(&admin.to_string()).is_err());
}
#[test]
fn execution_replies_keep_outcomes_cancellation_and_exact_values_distinct() {
    let update = json!({"jobId":"j","state":"finished","outcome":"unknown","cancellation":"confirmed","serverCancelSent":true,"deadlineExceeded":false,
        "statements":[{"index":0,"status":"succeeded","inTransaction":true,"affectedRows":"18446744073709551615","resultSets":1,"failure":null,"resultsWithheld":true},
                      {"index":1,"status":"failed","inTransaction":true,"affectedRows":"0","resultSets":0,"failure":{"constraint":"foreign_key","sqlState":"23503"},"resultsWithheld":false},
                      {"index":2,"status":"not_run","inTransaction":true,"affectedRows":"0","resultSets":0,"failure":null,"resultsWithheld":false}],
        "batch":{"statement":0,"result":0,"columns":[{"name":"n","databaseType":"NUMERIC","nullable":null}],"rows":[[{"kind":"decimal","text":"123456789012345678901234567890.4500"}]],"last":true,"truncated":false},
        "lease":"rollback_only","done":false});
    let decoded: JobUpdate = serde_json::from_value(update.clone()).unwrap();
    assert_eq!(serde_json::to_value(&decoded).unwrap(), update);
    assert!(serde_json::from_value::<JobUpdate>(json!({"state":"queued"})).is_err());
    let mut unflagged = update.clone();
    unflagged["statements"][0]
        .as_object_mut()
        .unwrap()
        .remove("resultsWithheld");
    assert!(
        serde_json::from_value::<JobUpdate>(unflagged).is_err(),
        "resultsWithheld is required so an interrupted statement's missing rows are always stated"
    );
    let end = json!({"outcome":"unknown","reason":"connection_lost","commitSent":false,"rollbackSent":true});
    assert_eq!(
        serde_json::to_value(serde_json::from_value::<TransactionEnd>(end.clone()).unwrap())
            .unwrap(),
        end
    );
    assert!(
        serde_json::from_value::<TransactionEnd>(
            json!({"outcome":"unknown","reason":"closed","commitSent":false})
        )
        .is_err(),
        "rollbackSent is required so an unknown outcome always states what was sent"
    );
    assert_eq!(
        ErrorDto::from(CoreError::rejected(ErrorCode::Busy)).code,
        ErrorCodeDto::Busy
    );
    let refused = ErrorDto::from(CoreError::rejected(ErrorCode::UnresolvedOutcomes));
    assert_eq!(
        serde_json::to_value(&refused).unwrap()["code"],
        json!("UNRESOLVED_OUTCOMES")
    );
    assert_eq!(refused.outcome, OutcomeDto::Rejected);
    let journal = ErrorDto::from(CoreError::rejected(ErrorCode::OutcomeJournalUnavailable));
    assert_eq!(
        (
            serde_json::to_value(&journal).unwrap()["code"].clone(),
            journal.outcome
        ),
        (json!("OUTCOME_JOURNAL_UNAVAILABLE"), OutcomeDto::Rejected)
    );
    let listed = json!({"outcomes":[
        {"outcomeId":"o","engine":"mariadb","database":"app","secondsAgo":3,"restored":true,"appExited":false,"transaction":{"outcome":"unknown","reason":"closed","commitSent":true,"rollbackSent":false},"job":null,"journalDamage":null},
        {"outcomeId":"d","engine":null,"database":"","secondsAgo":0,"restored":true,"appExited":false,"transaction":null,"job":null,"journalDamage":{"unreadableRecords":null,"preservedAs":"unknown-outcomes.journal.damaged-1-abcd"}}
    ],"limit":16,"pending":1,"journal":"write_failed"});
    assert_eq!(
        serde_json::to_value(serde_json::from_value::<UnresolvedOutcomes>(listed.clone()).unwrap())
            .unwrap(),
        listed
    );
    assert!(
        decode_envelope::<AcknowledgeOutcomeRequest>(
            &json!({"version":1,"requestId":"r","payload":{"sessionId":"s","outcomeId":"o"}})
                .to_string()
        )
        .is_ok()
    );
    let ack: Result<Envelope<AcknowledgeOutcomeRequest>, _> = decode_envelope(
        &json!({"version":1,"requestId":"r","payload":{"sessionId":"s","outcomeId":"o","outcome":"rolled_back"}}).to_string(),
    );
    assert!(
        ack.is_err(),
        "acknowledgement carries only the id; it can never set an outcome"
    );
}
