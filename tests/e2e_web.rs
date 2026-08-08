mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn e2e_setup_login_dashboard_logout() {
    let (app, _pool, _dir) = common::app_with_db().await;

    // GET / redirects to /setup on a fresh install.
    let res = app
        .clone()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    assert_eq!(res.headers().get("location").unwrap(), "/setup");

    // Create the first admin through the setup form.
    let form =
        "email=e2e%40example.com&password=Password123%21&password_confirmation=Password123%21";
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
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    assert_eq!(res.headers().get("location").unwrap(), "/login");

    // Log in via the API and capture the HttpOnly session cookie.
    let body = json!({"email": "e2e@example.com", "password": "Password123!"});
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
    assert_eq!(res.status(), StatusCode::OK);
    let cookie = res
        .headers()
        .get("set-cookie")
        .expect("login must set access_token cookie")
        .to_str()
        .unwrap()
        .to_string();
    assert!(cookie.starts_with("access_token="));
    assert!(cookie.contains("HttpOnly"));

    // GET / with the cookie renders the dashboard.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/")
                .header("Cookie", cookie.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(html.contains("Dashboard"));

    // Log out clears the cookie.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/logout")
                .header("Content-Type", "application/json")
                .body(Body::from(json!({"refresh_token": "unused"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let clear_cookie = res
        .headers()
        .get("set-cookie")
        .expect("logout must clear access_token cookie")
        .to_str()
        .unwrap();
    assert!(clear_cookie.contains("access_token="));
    assert!(clear_cookie.contains("Max-Age=0"));

    // GET / without a valid cookie is unauthorized again.
    let res = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}
