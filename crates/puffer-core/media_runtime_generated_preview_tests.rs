use super::*;
use tempfile::tempdir;

#[test]
fn generated_media_preview_by_artifact_uses_sidecar_path() {
    let workspace = tempdir().unwrap();
    let service = crate::runtime::media::MediaGenerationService::new(workspace.path());
    let image_path = service
        .write_image_artifact_bytes("artifact-1", "image.jpeg", &[0xff, 0xd8, 0xff, 0xd9])
        .unwrap();
    service
        .save_artifact(&crate::runtime::media::MediaArtifact {
            id: "artifact-1".to_string(),
            job_id: "job-1".to_string(),
            kind: crate::runtime::media::MediaKind::Image,
            path: image_path,
            mime_type: "image/jpeg".to_string(),
            byte_count: 4,
            metadata: serde_json::json!({}),
            created_at_ms: 1,
        })
        .unwrap();

    let result = read_generated_media_preview_by_artifact(workspace.path(), "artifact-1");

    assert_eq!(
        result,
        GeneratedMediaPreviewResult::Available {
            mime_type: "image/jpeg".to_string(),
            bytes: vec![0xff, 0xd8, 0xff, 0xd9],
        }
    );
}

#[test]
fn generated_media_preview_by_artifact_rejects_symlink_escape() {
    let workspace = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let outside_image = outside.path().join("image.jpeg");
    std::fs::write(&outside_image, [0xff, 0xd8, 0xff, 0xd9]).unwrap();
    let link_dir = workspace.path().join(".puffer/media/images/artifact-1");
    std::fs::create_dir_all(&link_dir).unwrap();
    let link = link_dir.join("image.jpeg");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside_image, &link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&outside_image, &link).unwrap();

    let service = crate::runtime::media::MediaGenerationService::new(workspace.path());
    service
        .save_artifact(&crate::runtime::media::MediaArtifact {
            id: "artifact-1".to_string(),
            job_id: "job-1".to_string(),
            kind: crate::runtime::media::MediaKind::Image,
            path: link,
            mime_type: "image/jpeg".to_string(),
            byte_count: 4,
            metadata: serde_json::json!({}),
            created_at_ms: 1,
        })
        .unwrap();

    assert_eq!(
        read_generated_media_preview_by_artifact(workspace.path(), "artifact-1"),
        GeneratedMediaPreviewResult::Unsupported
    );
}

#[test]
fn generated_media_attachment_metadata_rejects_symlink_escape() {
    let workspace = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let outside_image = outside.path().join("image.jpeg");
    std::fs::write(&outside_image, [0xff, 0xd8, 0xff, 0xd9]).unwrap();
    let link_dir = workspace.path().join(".puffer/media/images/artifact-1");
    std::fs::create_dir_all(&link_dir).unwrap();
    let link = link_dir.join("image.jpeg");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside_image, &link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&outside_image, &link).unwrap();

    let service = crate::runtime::media::MediaGenerationService::new(workspace.path());
    service
        .save_artifact(&crate::runtime::media::MediaArtifact {
            id: "artifact-1".to_string(),
            job_id: "job-1".to_string(),
            kind: crate::runtime::media::MediaKind::Image,
            path: link,
            mime_type: "image/jpeg".to_string(),
            byte_count: 4,
            metadata: serde_json::json!({}),
            created_at_ms: 1,
        })
        .unwrap();

    assert_eq!(
        generated_media_attachment_metadata(workspace.path(), "artifact-1"),
        None
    );
}

#[test]
fn generated_media_attachment_metadata_falls_back_when_sidecar_is_missing() {
    let workspace = tempdir().unwrap();

    let metadata = generated_media_attachment_metadata_with_fallback(
        workspace.path(),
        "artifact-1",
        "image/webp",
        42,
    )
    .unwrap();

    assert_eq!(metadata.artifact_id, "artifact-1");
    assert_eq!(metadata.mime_type, "image/webp");
    assert_eq!(metadata.byte_count, 42);
    assert_eq!(metadata.state, "missing");
}

#[test]
fn generated_media_attachment_metadata_fallback_does_not_bypass_unsafe_sidecar() {
    let workspace = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let outside_image = outside.path().join("image.jpeg");
    std::fs::write(&outside_image, [0xff, 0xd8, 0xff, 0xd9]).unwrap();
    let link_dir = workspace.path().join(".puffer/media/images/artifact-1");
    std::fs::create_dir_all(&link_dir).unwrap();
    let link = link_dir.join("image.jpeg");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside_image, &link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&outside_image, &link).unwrap();

    let service = crate::runtime::media::MediaGenerationService::new(workspace.path());
    service
        .save_artifact(&crate::runtime::media::MediaArtifact {
            id: "artifact-1".to_string(),
            job_id: "job-1".to_string(),
            kind: crate::runtime::media::MediaKind::Image,
            path: link,
            mime_type: "image/jpeg".to_string(),
            byte_count: 4,
            metadata: serde_json::json!({}),
            created_at_ms: 1,
        })
        .unwrap();

    assert_eq!(
        generated_media_attachment_metadata_with_fallback(
            workspace.path(),
            "artifact-1",
            "image/webp",
            42,
        ),
        None
    );
}

#[test]
fn generated_media_attachment_metadata_fallback_does_not_mask_corrupt_sidecar() {
    let workspace = tempdir().unwrap();
    let sidecar_dir = workspace.path().join(".puffer/media/artifact-sidecars");
    std::fs::create_dir_all(&sidecar_dir).unwrap();
    std::fs::write(sidecar_dir.join("artifact-1.json"), b"{not-json").unwrap();

    assert_eq!(
        generated_media_attachment_metadata_with_fallback(
            workspace.path(),
            "artifact-1",
            "image/webp",
            42,
        ),
        None
    );
}

#[test]
fn generated_media_preview_by_artifact_sniffs_mime_when_extension_lies() {
    let workspace = tempdir().unwrap();
    let service = crate::runtime::media::MediaGenerationService::new(workspace.path());
    let image_path = service
        .write_image_artifact_bytes("artifact-1", "image.png", &[0xff, 0xd8, 0xff, 0xd9])
        .unwrap();
    service
        .save_artifact(&crate::runtime::media::MediaArtifact {
            id: "artifact-1".to_string(),
            job_id: "job-1".to_string(),
            kind: crate::runtime::media::MediaKind::Image,
            path: image_path,
            mime_type: "image/png".to_string(),
            byte_count: 4,
            metadata: serde_json::json!({}),
            created_at_ms: 1,
        })
        .unwrap();

    let result = read_generated_media_preview_by_artifact(workspace.path(), "artifact-1");

    assert!(matches!(
        result,
        GeneratedMediaPreviewResult::Available { mime_type, .. } if mime_type == "image/jpeg"
    ));
}

#[test]
fn generated_video_access_metadata_accepts_video_under_artifact_root() {
    let workspace = tempdir().unwrap();
    let service = crate::runtime::media::MediaGenerationService::new(workspace.path());
    let video_path = service
        .write_artifact_bytes("artifact-video-1", "generated.mp4", b"mp4-bytes")
        .unwrap();
    service
        .save_artifact(&crate::runtime::media::MediaArtifact {
            id: "artifact-video-1".to_string(),
            job_id: "job-video-1".to_string(),
            kind: crate::runtime::media::MediaKind::Video,
            path: video_path.clone(),
            mime_type: "video/mp4".to_string(),
            byte_count: 9,
            metadata: serde_json::json!({}),
            created_at_ms: 1,
        })
        .unwrap();

    let result =
        generated_video_access_metadata_by_artifact(workspace.path(), "artifact-video-1");

    let GeneratedVideoAccessMetadataResult::Available(metadata) = result else {
        panic!("expected available video metadata");
    };
    assert_eq!(metadata.mime_type, "video/mp4");
    assert_eq!(metadata.byte_count, 9);
    assert_eq!(metadata.path, video_path.canonicalize().unwrap());
}

#[test]
fn generated_video_access_metadata_rejects_non_video_artifact() {
    let workspace = tempdir().unwrap();
    let service = crate::runtime::media::MediaGenerationService::new(workspace.path());
    let image_path = service
        .write_image_artifact_bytes("artifact-image-1", "image.jpeg", &[0xff, 0xd8, 0xff, 0xd9])
        .unwrap();
    service
        .save_artifact(&crate::runtime::media::MediaArtifact {
            id: "artifact-image-1".to_string(),
            job_id: "job-image-1".to_string(),
            kind: crate::runtime::media::MediaKind::Image,
            path: image_path,
            mime_type: "image/jpeg".to_string(),
            byte_count: 4,
            metadata: serde_json::json!({}),
            created_at_ms: 1,
        })
        .unwrap();

    assert_eq!(
        generated_video_access_metadata_by_artifact(workspace.path(), "artifact-image-1"),
        GeneratedVideoAccessMetadataResult::Unsupported
    );
}

#[test]
fn generated_video_access_metadata_rejects_symlink_escape() {
    let workspace = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let outside_video = outside.path().join("generated.mp4");
    std::fs::write(&outside_video, b"mp4-bytes").unwrap();
    let link_dir = workspace
        .path()
        .join(".puffer/media/artifacts/artifact-video-1");
    std::fs::create_dir_all(&link_dir).unwrap();
    let link = link_dir.join("generated.mp4");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside_video, &link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&outside_video, &link).unwrap();

    let service = crate::runtime::media::MediaGenerationService::new(workspace.path());
    service
        .save_artifact(&crate::runtime::media::MediaArtifact {
            id: "artifact-video-1".to_string(),
            job_id: "job-video-1".to_string(),
            kind: crate::runtime::media::MediaKind::Video,
            path: link,
            mime_type: "video/mp4".to_string(),
            byte_count: 9,
            metadata: serde_json::json!({}),
            created_at_ms: 1,
        })
        .unwrap();

    assert_eq!(
        generated_video_access_metadata_by_artifact(workspace.path(), "artifact-video-1"),
        GeneratedVideoAccessMetadataResult::Unsupported
    );
}
