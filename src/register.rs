use std::process::exit;

use inquire::{validator::MinLengthValidator, Password, PasswordDisplayMode, Text};
use IAMService::{
    apis::{
        configuration::Configuration,
        default_api::{identity_register, IdentityRegisterParams},
    },
    models::RegisterRequest,
};

pub async fn register(iam_config: Configuration) {
    match Text::new("Whats your email ID")
        .with_validator(MinLengthValidator::new(1))
        .prompt()
    {
        Ok(user_id) => {
            let password = match Password::new("Enter password:")
                .with_display_mode(PasswordDisplayMode::Masked)
                .prompt()
            {
                Ok(p) => p,
                Err(_) => {
                    println!("You cancelled, cant proceed without the password");
                    exit(1);
                }
            };
            match identity_register(
                &iam_config,
                IdentityRegisterParams {
                    register_request: RegisterRequest {
                        email: user_id,
                        password,
                    },
                },
            )
            .await
            {
                Ok(register_response) => {
                    println!("{:?}", register_response);
                }
                Err(e) => {
                    println!("{:?}", e)
                }
            };
            // println!("{:?}, {:?}", user_id, password);
        }
        Err(_) => {
            println!("We can not proceed without your email ID, please try again if you change your mind");
            exit(1);
        }
    };
}
