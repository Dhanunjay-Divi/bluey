//! Integration test: mic device selection flows from settings through IPC to
//! the audio capture config.

use cue_core::ipc::DaemonRequest;
use cue_core::AudioCaptureConfig;

/// Verify that `DaemonRequest::AudioStart` carries `mic_device_id` and that
/// constructing an `AudioCaptureConfig` from it populates the microphone
/// device_id field correctly (the same logic the daemon handler uses).
#[test]
fn audio_start_request_carries_mic_device_id() {
    let device = "My USB Microphone".to_string();
    let request = DaemonRequest::AudioStart {
        enable_system: false,
        enable_microphone: true,
        mic_device_id: Some(device.clone()),
    };

    // Simulate what the daemon handler does:
    if let DaemonRequest::AudioStart {
        enable_system,
        enable_microphone,
        mic_device_id,
    } = request
    {
        let mut config = AudioCaptureConfig::from_enabled_sources(enable_system, enable_microphone);
        if let Some(id) = mic_device_id {
            config.microphone.device_id = Some(id);
        }

        assert!(!config.system.enabled);
        assert!(config.microphone.enabled);
        assert_eq!(config.microphone.device_id, Some(device));
    } else {
        panic!("expected AudioStart");
    }
}

/// Verify that `mic_device_id: None` leaves the config device_id as None
/// (default device behavior).
#[test]
fn audio_start_request_none_device_uses_default() {
    let request = DaemonRequest::AudioStart {
        enable_system: true,
        enable_microphone: true,
        mic_device_id: None,
    };

    if let DaemonRequest::AudioStart {
        enable_system,
        enable_microphone,
        mic_device_id,
    } = request
    {
        let mut config = AudioCaptureConfig::from_enabled_sources(enable_system, enable_microphone);
        if let Some(id) = mic_device_id {
            config.microphone.device_id = Some(id);
        }

        assert!(config.system.enabled);
        assert!(config.microphone.enabled);
        assert_eq!(config.microphone.device_id, None);
    } else {
        panic!("expected AudioStart");
    }
}

/// Verify that `load_mic_device_setting` reads from the DB correctly.
/// This exercises the full path: save_setting -> load_mic_device_setting.
#[test]
fn load_mic_device_setting_round_trips() {
    use cue_daemon::audio::capture::load_mic_device_setting;
    use cue_daemon::db::Database;

    let db_path = format!("/tmp/cue_test_mic_device_{}.db", std::process::id());
    let db = Database::open(&db_path).unwrap();
    db.save_setting("audio.mic_device", "Blue Yeti").unwrap();
    drop(db);

    let loaded = load_mic_device_setting(&db_path);
    assert_eq!(loaded, Some("Blue Yeti".to_string()));

    // Cleanup
    let _ = std::fs::remove_file(&db_path);
}

/// Verify the IPC serialization round-trips with the new field.
#[test]
fn audio_start_ipc_serialization_with_device() {
    let request = DaemonRequest::AudioStart {
        enable_system: false,
        enable_microphone: true,
        mic_device_id: Some("Rode NT-USB".to_string()),
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("mic_device_id"));
    assert!(json.contains("Rode NT-USB"));

    let deserialized: DaemonRequest = serde_json::from_str(&json).unwrap();
    if let DaemonRequest::AudioStart { mic_device_id, .. } = deserialized {
        assert_eq!(mic_device_id, Some("Rode NT-USB".to_string()));
    } else {
        panic!("deserialization failed");
    }
}

/// Verify backward compatibility: JSON without mic_device_id deserializes
/// with the field as None (serde default for Option).
#[test]
fn audio_start_ipc_backward_compat_missing_field() {
    let json = r#"{"type":"audio_start","enable_system":true,"enable_microphone":true}"#;
    let request: DaemonRequest = serde_json::from_str(json).unwrap();
    if let DaemonRequest::AudioStart { mic_device_id, .. } = request {
        assert_eq!(mic_device_id, None);
    } else {
        panic!("deserialization failed");
    }
}
