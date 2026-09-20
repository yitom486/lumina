use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use lumina_acp::jobs::isolated::ChapterSession;
use lumina_acp::{
    AgentKind, AgentProfileInput, AgentProfilesHint, SessionEnvironment, SessionKind,
};
use serde_json::{json, Value};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!(
            "lumina-acp-chapter-session-blackbox-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path)?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct TestSessionEnvironment {
    root: PathBuf,
}

impl SessionEnvironment for TestSessionEnvironment {
    fn snapshot_path(&self, _cwd: &Path, kind: SessionKind) -> PathBuf {
        self.root.join(format!("snapshot-{}.json", kind.as_str()))
    }

    fn sync_snapshot(&self, snapshot_path: &Path, vision_capable: bool) -> Result<(), String> {
        fs::write(
            snapshot_path,
            json!({ "test": true, "visionCapable": vision_capable }).to_string(),
        )
        .map_err(|error| error.to_string())
    }

    fn mcp_servers(&self, _snapshot_path: &Path, _kind: SessionKind) -> Value {
        json!([])
    }

    fn snapshot_vision_capable(&self, _workspace: &Path) -> Option<bool> {
        None
    }
}

fn test_profiles(state_path: &Path) -> Result<AgentProfilesHint, Box<dyn std::error::Error>> {
    let mock_agent = mock_agent_path()?;
    Ok(AgentProfilesHint {
        active_profile_id: "mock".into(),
        profiles: vec![AgentProfileInput {
            id: "mock".into(),
            name: "测试 ACP Agent".into(),
            kind: AgentKind::Custom,
            command: mock_agent,
            args: Vec::new(),
            env: HashMap::from([(
                "LUMINA_ACP_MOCK_STATE".into(),
                state_path.to_string_lossy().into_owned(),
            )]),
            launcher: None,
            env_preset: None,
            auth_policy: None,
            auth_methods: Vec::new(),
            session_storage: None,
        }],
    })
}

fn mock_agent_path() -> Result<String, Box<dyn std::error::Error>> {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_lumina_acp_test_mock_agent") {
        return Ok(path);
    }
    let mut path = std::env::current_exe()?;
    path.pop();
    path.pop();
    path.push(format!(
        "lumina-acp-test-mock-agent{}",
        std::env::consts::EXE_SUFFIX
    ));
    if path.is_file() {
        return Ok(path.to_string_lossy().into_owned());
    }
    Err(format!("test mock agent binary not found at {}", path.display()).into())
}

#[test]
fn chapter_session_recovers_after_stdio_eof_on_a_fresh_spawn(
) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = TestDirectory::new()?;
    let state_path = fixture.path().join("spawn-count.txt");
    assert!(lumina_acp::set_default_environment(Arc::new(
        TestSessionEnvironment {
            root: fixture.path().to_path_buf(),
        }
    )));

    let profiles = test_profiles(&state_path)?;
    let session = ChapterSession::new(
        Some(fixture.path().to_string_lossy().into_owned()),
        "mock".into(),
        profiles,
        None,
        Some("h-4-transport-recovery".into()),
    );

    let first_reply = session.prompt("第一次 prompt：模拟传输中断")?;
    assert!(first_reply.contains("未解析到文本回复"));
    assert!(!first_reply.contains("mock-agent"));
    assert!(!first_reply.contains("stderr"));
    assert_eq!(fs::read_to_string(&state_path)?.trim(), "1");

    let recovered = session.prompt("第二次 prompt：验证恢复")?;
    assert_eq!(recovered, "恢复成功：mock ACP");
    assert_eq!(fs::read_to_string(&state_path)?.trim(), "2");
    session.close()?;
    Ok(())
}
