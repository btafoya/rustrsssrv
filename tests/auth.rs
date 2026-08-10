mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

async fn create_user(app: &axum::Router, email: &str, password: &str) {
    let form = format!(
        "email={}&password={}&password_confirmation={}",
        urlencoding::encode(email),
        urlencoding::encode(password),
        urlencoding::encode(password)
    );
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/setup")
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(Body::from(form))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(res.status().is_success() || res.status() == StatusCode::SEE_OTHER);
}

async fn login(app: &axum::Router, email: &str, password: &str) -> (StatusCode, serde_json::Value) {
    let body = json!({"email": email, "password": password});
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(json!({}));
    (status, json)
}

#[tokio::test]
async fn login_with_valid_credentials() {
    let (app, _pool, _dir) = common::app_with_db().await;
    create_user(&app, "user@example.com", "Password123!").await;

    let (status, json) = login(&app, "user@example.com", "Password123!").await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["access_token"].as_str().is_some());
    assert!(json["refresh_token"].as_str().is_some());
}

#[tokio::test]
async fn login_with_invalid_credentials() {
    let (app, _pool, _dir) = common::app_with_db().await;
    create_user(&app, "user@example.com", "Password123!").await;

    let (status, _json) = login(&app, "user@example.com", "wrongpassword").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn refresh_and_logout_lifecycle() {
    let (app, _pool, _dir) = common::app_with_db().await;
    create_user(&app, "user@example.com", "Password123!").await;

    let (_status, login_json) = login(&app, "user@example.com", "Password123!").await;
    let refresh_token = login_json["refresh_token"].as_str().unwrap().to_string();

    let body = json!({"refresh_token": refresh_token});
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/refresh")
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let refresh_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(refresh_json["access_token"].as_str().is_some());

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/logout")
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // Refresh should fail after logout
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/refresh")
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn users_me_requires_auth() {
    let (app, _pool, _dir) = common::app_with_db().await;

    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/users/me")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn users_me_returns_profile() {
    let (app, _pool, _dir) = common::app_with_db().await;
    create_user(&app, "user@example.com", "Password123!").await;

    let (_status, login_json) = login(&app, "user@example.com", "Password123!").await;
    let access_token = login_json["access_token"].as_str().unwrap();

    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/users/me")
                .header("Authorization", format!("Bearer {}", access_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["email"], "user@example.com");
}

async fn patch_me(
    app: &axum::Router,
    access_token: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/users/me")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", access_token))
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(json!({}));
    (status, json)
}

#[tokio::test]
async fn change_password_requires_current_password() {
    let (app, _pool, _dir) = common::app_with_db().await;
    create_user(&app, "user@example.com", "Password123!").await;
    let (_status, login_json) = login(&app, "user@example.com", "Password123!").await;
    let access_token = login_json["access_token"].as_str().unwrap();

    let (status, _json) = patch_me(
        &app,
        access_token,
        json!({"new_password": "NewPassword456$"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn change_password_with_wrong_current_password_is_rejected() {
    let (app, _pool, _dir) = common::app_with_db().await;
    create_user(&app, "user@example.com", "Password123!").await;
    let (_status, login_json) = login(&app, "user@example.com", "Password123!").await;
    let access_token = login_json["access_token"].as_str().unwrap();

    let (status, _json) = patch_me(
        &app,
        access_token,
        json!({"current_password": "wrongpassword", "new_password": "NewPassword456$"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Password must remain unchanged.
    let (status, _json) = login(&app, "user@example.com", "Password123!").await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn change_password_success_revokes_other_sessions() {
    let (app, _pool, _dir) = common::app_with_db().await;
    create_user(&app, "user@example.com", "Password123!").await;
    let (_status, login_json) = login(&app, "user@example.com", "Password123!").await;
    let access_token = login_json["access_token"].as_str().unwrap().to_string();
    let refresh_token = login_json["refresh_token"].as_str().unwrap().to_string();

    let (status, _json) = patch_me(
        &app,
        &access_token,
        json!({"current_password": "Password123!", "new_password": "NewPassword456$"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Old refresh token from before the password change must be revoked.
    let refresh_body = json!({"refresh_token": refresh_token});
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/refresh")
                .header("Content-Type", "application/json")
                .body(Body::from(refresh_body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // New password logs in; old password no longer works.
    let (status, _json) = login(&app, "user@example.com", "NewPassword456$").await;
    assert_eq!(status, StatusCode::OK);
    let (status, _json) = login(&app, "user@example.com", "Password123!").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
