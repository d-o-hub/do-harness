#!/usr/bin/env python3
"""Validate normalized packages and installed dependency provenance in isolation.

The packaged mode stages actual cargo package archives in a directory registry.
Only registry dependencies are rewritten by Cargo; no path/patch is injected into
the normalized manifests. The registry mode checks the actual published crates.
Both use a fresh Cargo home, an out-of-checkout cwd and the same CLI smoke test.
"""

import argparse
import hashlib
import json
import os
import pathlib
import subprocess
import tarfile
import tempfile
import tomllib

from smoke_db_distribution import smoke

ROOT = pathlib.Path(__file__).resolve().parents[1]
POLICY = ROOT / "scripts/fixtures/distribution/libsql.json"


def read_toml(path):
    return tomllib.loads(path.read_text(encoding="utf-8"))


def require(condition, message):
    if not condition:
        raise ValueError(message)


def run(args, cwd, env, capture=False):
    print("+ " + " ".join(map(str, args)), flush=True)
    return subprocess.run(list(map(str, args)), cwd=cwd, env=env, check=True,
                          text=True, stdout=subprocess.PIPE if capture else None).stdout


def check_fork(path, policy):
    manifest = read_toml(path / "Cargo.toml")
    package = manifest["package"]
    require(package["name"] == policy["package"], package)
    require(package["version"] == policy["version"], package)
    require(package["license"] == "MIT" and (path / "LICENSE").is_file(), "MIT license missing")
    require(package["metadata"]["vendor-fork"]["upstream-ref"] == "libsql-0.9.30", "upstream provenance changed")
    digest = hashlib.sha256((path / "src/local/connection.rs").read_bytes()).hexdigest()
    require(digest == policy["connection_sha256"], "libSQL fix source digest changed")
    require(not any(key in manifest for key in ("patch", "replace", "workspace")), "manifest was not normalized")
    return digest


def check_db_manifest(path, policy):
    manifest = read_toml(path / "Cargo.toml")
    dependency = manifest["dependencies"]["libsql"]
    require(dependency.get("package") == policy["package"], dependency)
    require(dependency["version"] == policy["version"], dependency)
    require("core" in dependency["features"] and not dependency["default-features"], dependency)
    require(not any(key in manifest for key in ("patch", "replace", "workspace")), "manifest was not normalized")
    for group in ("dependencies", "build-dependencies"):
        for dependency in manifest.get(group, {}).values():
            require(not any(key in dependency for key in ("path", "git", "workspace")), dependency)


def stage_archive(archive, destination):
    with tarfile.open(archive) as tar:
        # These archives were produced by Cargo, but still reject path escapes.
        tar.extractall(destination, filter="data")
        top = destination / tar.getnames()[0].split("/")[0]
    files = {str(path.relative_to(top)).replace(os.sep, "/"):
             hashlib.sha256(path.read_bytes()).hexdigest()
             for path in top.rglob("*") if path.is_file()}
    (top / ".cargo-checksum.json").write_text(json.dumps({
        "files": files, "package": hashlib.sha256(archive.read_bytes()).hexdigest()
    }), encoding="utf-8")
    return top


def check_metadata(metadata, policy, expected_version):
    packages = metadata["packages"]
    # Reject a parallel unpatched implementation, even if the CLI smoke passed.
    require(not any(p["name"] == "libsql" for p in packages), "unpatched libSQL resolved")
    fork = [p for p in packages if p["name"] == policy["package"]]
    require(len(fork) == 1 and fork[0]["version"] == policy["version"], fork)
    fork = fork[0]
    require(fork["source"] == "registry+https://github.com/rust-lang/crates.io-index", fork)
    db = [p for p in packages if p["name"] == "do-harness-db"]
    require(len(db) == 1 and db[0]["version"] == expected_version, db)
    require(db[0]["source"] == fork["source"], db)
    node = next(n for n in metadata["resolve"]["nodes"] if n["id"] == db[0]["id"])
    require(any(d["pkg"] == fork["id"] and d["name"] == "libsql" for d in node["deps"]), "db does not resolve the approved fork")
    digest = check_fork(pathlib.Path(fork["manifest_path"]).parent, policy)
    check_db_manifest(pathlib.Path(db[0]["manifest_path"]).parent, policy)
    return {"dependency": fork["name"], "dependency_version": fork["version"],
            "dependency_source": fork["source"], "connection_sha256": digest}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode", choices=("packaged", "registry"), default="packaged")
    parser.add_argument("--target")
    parser.add_argument("--check-policy", action="store_true")
    parser.add_argument("--prebuilt", type=pathlib.Path)
    args = parser.parse_args()
    policy = json.loads(POLICY.read_text(encoding="utf-8"))
    workspace = read_toml(ROOT / "Cargo.toml")
    version = workspace["workspace"]["package"]["version"]
    dependency = workspace["workspace"]["dependencies"]["libsql"]
    require(dependency.get("package") == policy["package"], dependency)
    require(dependency["version"] == policy["version"], dependency)
    require(not any(key in workspace for key in ("patch", "replace")), "workspace override hides source distribution drift")
    check_fork(ROOT / "vendor/libsql", policy)
    if args.check_policy:
        print("libSQL distribution policy OK.")
        return
    if not args.target:
        parser.error("--target is required unless --check-policy is used")
    # Keep Rust tool selection, but exclude all ambient Cargo overrides (including
    # CARGO_ENCODED_RUSTFLAGS, CARGO_BUILD_TARGET and registry/source settings).
    env = {k: v for k, v in os.environ.items()
           if not k.startswith("CARGO_") and k not in ("RUSTFLAGS", "RUSTDOCFLAGS")}
    env["RUSTUP_TOOLCHAIN"] = read_toml(ROOT / "rust-toolchain.toml")["toolchain"]["channel"]
    toolchain = run(["rustc", "-Vv"], ROOT, env, capture=True)
    with tempfile.TemporaryDirectory(prefix="do-harness-distribution-") as scratch:
        work = pathlib.Path(scratch).resolve()
        require(not work.is_relative_to(ROOT), "distribution cwd is inside the checkout")
        for ancestor in (work, *work.parents):
            for name in ("config", "config.toml"):
                require(not (ancestor / ".cargo" / name).exists(),
                        f"ambient Cargo configuration in {ancestor}")
        home = work / "cargo-home"
        home.mkdir()
        isolated = dict(env, CARGO_HOME=str(home), CARGO_TARGET_DIR=str(work / "target"),
                        CARGO_BUILD_JOBS="2")
        cli = None
        if args.mode == "packaged":
            # Bootstrap uses the download cache only to prepare registry sources.
            # Every package/install/metadata operation below runs isolated.
            fork_manifest = ROOT / "vendor/libsql/Cargo.toml"
            run(["cargo", "package", "--manifest-path", fork_manifest, "--allow-dirty",
                 "--locked", "--no-verify", "--no-default-features", "--features", "core"], ROOT, env)
            registry = work / "registry"
            run(["cargo", "vendor", "--locked", "--versioned-dirs", "--sync",
                 fork_manifest, registry], ROOT, env)
            (home / "config.toml").write_text(
                '[source.crates-io]\nreplace-with = "distribution-test"\n'
                '[source.distribution-test]\ndirectory = ' + json.dumps(str(registry)) + '\n',
                encoding="utf-8")
            fork = stage_archive(ROOT / f"vendor/libsql/target/package/{policy['package']}-{policy['version']}.crate", registry)
            check_fork(fork, policy)
            for crate, manifest in (("do-harness-types", "crates/types/Cargo.toml"),
                                    ("do-harness-db", "crates/db/Cargo.toml"),
                                    ("do-harness", "crates/do-harness/Cargo.toml")):
                run(["cargo", "package", "--manifest-path", ROOT / manifest, "--locked",
                     "--allow-dirty", "--no-verify"], work, isolated)
                package = stage_archive(work / f"target/package/{crate}-{version}.crate", registry)
                if crate == "do-harness-db":
                    check_db_manifest(package, policy)
                if crate == "do-harness":
                    cli = package
            install = ["--path", str(cli)]
        else:
            install = ["do-harness", "--version", "=" + version]
        run(["cargo", "install", *install, "--locked", "--target", args.target,
             "--root", work / "installed"], work, isolated)
        if cli is None:
            matches = list(home.glob(f"registry/src/*/do-harness-{version}/Cargo.toml"))
            require(len(matches) == 1, matches)
            cli = matches[0].parent
        metadata = json.loads(run(["cargo", "metadata", "--manifest-path", cli / "Cargo.toml",
                                   "--locked", "--format-version", "1", "--filter-platform",
                                   args.target], work, isolated, capture=True))
        provenance = check_metadata(metadata, policy, version)
        print(json.dumps(dict(provenance, mode=args.mode, artifact_version=version,
                              target=args.target, toolchain=toolchain,
                              check="dependency-provenance", status="pass")), flush=True)
        binary = work / "installed/bin" / ("do-harness.exe" if "windows" in args.target else "do-harness")
        smoke(binary, expected_version=version)
        if args.prebuilt:
            smoke(args.prebuilt, expected_version=version)


if __name__ == "__main__":
    main()
