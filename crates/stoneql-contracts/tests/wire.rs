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
