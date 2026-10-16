use super::*;

#[test]
fn auth_json_roundtrip_via_tokens_shape() {
    let auth = AuthDotJson::chatgpt(
        TokenData {
            id_token: "id".into(),
            access_token: "access".into(),
            refresh_token: "refresh".into(),
            account_id: Some("acct_1".into()),
        },
        None,
    );
    let tokens = auth.to_supplier_tokens("row-1");
    let loaded = AuthDotJson::from_supplier_tokens(&tokens).unwrap();
    assert_eq!(loaded.auth_mode.as_deref(), Some("chatgpt"));
    assert_eq!(loaded.chatgpt_account_id(), Some("acct_1"));
    assert_eq!(loaded.tokens.unwrap().access_token, "access");
}
