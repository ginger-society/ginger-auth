use IAMService::{apis::default_api::identity_validate_token, get_configuration};

pub async fn get_session_info() {
    let iam_config = get_configuration(None);
    println!("{:?}", iam_config);
    match identity_validate_token(&iam_config).await {
        Ok(profile_response) => {
            println!("{:?}", profile_response)
        }
        Err(e) => {
            println!("{:?}", e);
        }
    };
}
