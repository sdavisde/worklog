//! Integration tests: invoke the built `wl` binary as a subprocess against a
//! temp `WORKLOG_DIR`. Every test below sets `WORKLOG_DIR` explicitly and
//! must never touch a real `~/.worklog`.

use chrono::{Duration, Local};
use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_wl")
}

fn wl(dir: &std::path::Path) -> Command {
    let mut cmd = Command::new(bin());
    cmd.env("WORKLOG_DIR", dir);
    cmd
}

#[test]
fn task_capture_writes_correct_jsonl_fields() {
    let dir = tempdir().unwrap();

    let output = wl(dir.path())
        .args([
            "task",
            "demo task",
            "--category",
            "engineering",
            "--project",
            "auth-revamp",
            "--due",
            "2026-07-10",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(dir.path().join("tasks.jsonl")).unwrap();
    let line = content
        .lines()
        .next()
        .expect("tasks.jsonl should have one line");
    let value: serde_json::Value = serde_json::from_str(line).unwrap();

    assert_eq!(value["text"], "demo task");
    assert_eq!(value["category"], "engineering");
    assert_eq!(value["project"], "auth-revamp");
    assert_eq!(value["status"], "open");
    assert_eq!(value["due"], "2026-07-10");
    assert!(value["completed_at"].is_null());
    assert!(
        value["id"].as_str().unwrap().starts_with("t_"),
        "id was {:?}",
        value["id"]
    );
    assert!(
        value["created_at"].as_str().unwrap().contains('T'),
        "created_at should be RFC3339: {:?}",
        value["created_at"]
    );
}

#[test]
fn task_capture_defaults_category_to_intake() {
    let dir = tempdir().unwrap();

    let output = wl(dir.path())
        .args(["task", "no category given"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let content = fs::read_to_string(dir.path().join("tasks.jsonl")).unwrap();
    let value: serde_json::Value = serde_json::from_str(content.lines().next().unwrap()).unwrap();
    assert_eq!(value["category"], "intake");
    assert!(value["project"].is_null());
    assert!(value["due"].is_null());
}

#[test]
fn invalid_category_is_rejected() {
    let dir = tempdir().unwrap();

    let output = wl(dir.path())
        .args(["task", "demo", "--category", "not-a-real-category"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        !dir.path().join("tasks.jsonl").exists(),
        "no task should have been written on validation failure"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not-a-real-category"), "stderr: {stderr}");
    // Error message should list the valid categories so the user can fix it.
    assert!(stderr.contains("intake"), "stderr: {stderr}");
}

#[test]
fn config_yaml_is_auto_created_on_first_use() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.yaml");
    assert!(!config_path.exists());

    let output = wl(dir.path())
        .args(["task", "trigger config creation"])
        .output()
        .unwrap();
    assert!(output.status.success());

    assert!(config_path.exists());
    let content = fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("categories:"));
    assert!(content.contains("intake"));
    assert!(content.contains("editor_command: nvim"));
}

#[test]
fn standup_groups_open_blocked_and_completed_with_fallback_labeling() {
    let dir = tempdir().unwrap();

    // Seed tasks.jsonl with one open and one blocked task.
    let now = Local::now().fixed_offset().to_rfc3339();
    let tasks_jsonl = format!(
        "{{\"id\":\"t_open001\",\"text\":\"Open task item\",\"category\":\"intake\",\"project\":null,\"status\":\"open\",\"due\":null,\"created_at\":\"{now}\",\"completed_at\":null}}\n\
         {{\"id\":\"t_block001\",\"text\":\"Blocked task item\",\"category\":\"support\",\"project\":null,\"status\":\"blocked\",\"due\":null,\"created_at\":\"{now}\",\"completed_at\":null}}\n"
    );
    fs::write(dir.path().join("tasks.jsonl"), tasks_jsonl).unwrap();

    // Seed archive.jsonl with a completion from 3 days ago (no completion
    // yesterday), so standup must fall back to labeling the most recent day
    // with completions.
    let three_days_ago = (Local::now() - Duration::days(3)).fixed_offset();
    let three_days_ago_str = three_days_ago.to_rfc3339();
    let archive_jsonl = format!(
        "{{\"id\":\"t_done0001\",\"text\":\"Completed a while back\",\"category\":\"engineering\",\"project\":null,\"status\":\"done\",\"due\":null,\"created_at\":\"{three_days_ago_str}\",\"completed_at\":\"{three_days_ago_str}\"}}\n"
    );
    fs::write(dir.path().join("archive.jsonl"), archive_jsonl).unwrap();

    let output = wl(dir.path()).arg("standup").output().unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    let expected_date = three_days_ago.date_naive().format("%Y-%m-%d").to_string();
    assert!(stdout.contains("most recent"), "stdout: {stdout}");
    assert!(stdout.contains(&expected_date), "stdout: {stdout}");
    assert!(
        stdout.contains("Completed a while back"),
        "stdout: {stdout}"
    );

    assert!(stdout.contains("Today"), "stdout: {stdout}");
    assert!(stdout.contains("Open task item"), "stdout: {stdout}");

    assert!(stdout.contains("Blocked"), "stdout: {stdout}");
    assert!(stdout.contains("Blocked task item"), "stdout: {stdout}");
}

#[test]
fn standup_shows_todays_completions_under_today_not_yesterday() {
    let dir = tempdir().unwrap();

    let now = Local::now().fixed_offset().to_rfc3339();
    let tasks_jsonl = format!(
        "{{\"id\":\"t_open001\",\"text\":\"Still open item\",\"category\":\"intake\",\"project\":null,\"status\":\"open\",\"due\":null,\"created_at\":\"{now}\",\"completed_at\":null}}\n"
    );
    fs::write(dir.path().join("tasks.jsonl"), tasks_jsonl).unwrap();

    // One completion today and one yesterday: today's must land in "Today",
    // yesterday's in the "Completed yesterday" section — never duplicated.
    let yesterday = (Local::now() - Duration::days(1))
        .fixed_offset()
        .to_rfc3339();
    let archive_jsonl = format!(
        "{{\"id\":\"t_today001\",\"text\":\"Finished today item\",\"category\":\"engineering\",\"project\":null,\"status\":\"done\",\"due\":null,\"created_at\":\"{now}\",\"completed_at\":\"{now}\"}}\n\
         {{\"id\":\"t_yest0001\",\"text\":\"Finished yesterday item\",\"category\":\"engineering\",\"project\":null,\"status\":\"done\",\"due\":null,\"created_at\":\"{yesterday}\",\"completed_at\":\"{yesterday}\"}}\n"
    );
    fs::write(dir.path().join("archive.jsonl"), archive_jsonl).unwrap();

    let output = wl(dir.path()).arg("standup").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    // "Finished today item" appears exactly once, and after the "Today"
    // heading rather than under "Completed yesterday".
    let today_heading = stdout.find("Today").expect("Today heading present");
    let yesterday_section = stdout
        .find("Completed yesterday")
        .expect("yesterday section");
    let today_item = stdout
        .find("Finished today item")
        .expect("today completion");
    let open_item = stdout.find("Still open item").expect("open item");
    assert_eq!(stdout.matches("Finished today item").count(), 1, "no dup");
    assert!(yesterday_section < today_heading, "sections ordered");
    assert!(today_heading < today_item, "today completion under Today");
    assert!(today_heading < open_item, "open item under Today");
    assert!(
        stdout.contains("Finished yesterday item"),
        "stdout: {stdout}"
    );
}

#[test]
fn standup_with_no_data_prints_empty_sections() {
    let dir = tempdir().unwrap();

    let output = wl(dir.path()).arg("standup").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("Completed yesterday"), "stdout: {stdout}");
    assert!(stdout.contains("Today"), "stdout: {stdout}");
    assert!(stdout.contains("Blocked"), "stdout: {stdout}");
    assert!(stdout.contains("(none)"), "stdout: {stdout}");
}
