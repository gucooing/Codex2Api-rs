mod common;
use codex2api_accounts::{
    AccountIdentity, AuthDotJson, HostRuntime, OauthIdentity, TokenData, new_installation_id,
};
use common::Fixture;

#[tokio::test]
async fn supplier_list_and_details_distinguish_members_and_personal_workspaces() {
    let f = Fixture::new().await;
    let mut saved = Vec::new();
    for (user, workspace, plan) in [
        ("alice", "team", "business"),
        ("bob", "team", "business"),
        ("alice", "personal", "pro"),
    ] {
        let identity = AccountIdentity::new(
            uuid::Uuid::new_v4().to_string(),
            new_installation_id(),
            HostRuntime::generate(),
        );
        let account = f
            .state
            .accounts
            .save_authorized_identity(
                &identity,
                OauthIdentity {
                    chatgpt_account_id: workspace.into(),
                    chatgpt_user_id: Some(user.into()),
                    email: Some(format!("{user}@example.test")),
                    plan_type: Some(plan.into()),
                    display_name: None,
                },
                &AuthDotJson::chatgpt(
                    TokenData {
                        access_token: "fixture".into(),
                        account_id: Some(workspace.into()),
                        ..Default::default()
                    },
                    None,
                ),
                None,
            )
            .await
            .unwrap()
            .account;
        let detail = f.get(&format!("/admin/api/suppliers/{}", account.id)).await;
        assert_eq!(detail["chatgpt_account_id"], workspace);
        assert_eq!(detail["chatgpt_user_id"], user);
        assert_eq!(detail["plan_type"], plan);
        assert_eq!(detail["installation_id"], identity.installation_id);
        saved.push(account.id);
    }
    let list = f.get("/admin/api/suppliers").await;
    let rows = list["items"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    for id in saved {
        assert_eq!(rows.iter().filter(|row| row["id"] == id).count(), 1);
    }
    f.storage.close().await;
}
