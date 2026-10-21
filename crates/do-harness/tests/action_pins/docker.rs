//! Docker image reference cases for the `action-pins` sensor.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::fixtures::{IMAGE_DIGEST, run, workflow_with, workspace, write_workflow};

/// A Docker action image pinned to a sha256 digest passes, with or without a
/// tag in front of the digest.
#[test]
fn docker_digest_passes() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/docker.yml",
        &format!(
            "name: docker\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: docker://alpine:3.20@{IMAGE_DIGEST}\n      - uses: docker://alpine@{IMAGE_DIGEST}\n      - uses: docker://ghcr.io/owner/image:1.0@{IMAGE_DIGEST}\n"
        ),
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 0, "a digest-pinned docker image must pass: {output}");
}

/// A digest that is not a sha256 — or is the wrong length — is mutable.
#[test]
fn malformed_docker_digest_fails() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/docker.yml",
        "name: docker\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: docker://alpine@sha256:deadbeef\n",
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "a malformed digest must fail: {output}");
    assert!(
        output.contains("sha256"),
        "the finding must name the digest requirement: {output}"
    );
}

/// Empty and traversal components are not valid Docker image names.
#[test]
fn malformed_docker_name_fails() {
    for image in ["alpine/", "alpine//sub", "../alpine", "alpine/../sub"] {
        let dir = workspace();
        let reference = format!("docker://{image}@{IMAGE_DIGEST}");
        write_workflow(
            dir.path(),
            ".github/workflows/docker.yml",
            &workflow_with(&reference),
        );

        let (code, output) = run(dir.path());
        assert_eq!(
            code, 1,
            "a malformed Docker image name must fail ({image}): {output}"
        );
        assert!(
            output.contains("valid image reference"),
            "the finding must name the malformed Docker image: {output}"
        );
    }
}

/// A Docker image pinned only by tag is mutable.
#[test]
fn mutable_docker_tag_fails() {
    let dir = workspace();
    write_workflow(
        dir.path(),
        ".github/workflows/docker.yml",
        "name: docker\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: docker://alpine:3.20\n",
    );

    let (code, output) = run(dir.path());
    assert_eq!(code, 1, "a docker tag must fail: {output}");
    assert!(
        output.contains("sha256"),
        "the finding must name the digest requirement: {output}"
    );
}
