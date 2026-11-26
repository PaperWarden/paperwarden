# Product-invariant checks

`scripts/check_invariants.py` checks the repository for changes that would
break `PRODUCT_INVARIANTS.md`. This page states what each check proves and,
just as important, what it does not. A passing run is evidence, not a proof
that the app is offline or free of tracking. Human review under
`SECURITY_REVIEW_PLAYBOOK.md` is still required.

Run it locally before a pull request that touches guardrails, dependencies,
permissions or platform code:

```sh
python -m unittest discover -s tests/scripts -v
python scripts/check_invariants.py
```

Inside a git checkout it reads **tracked files only**, which is what CI sees.
Untracked local files, such as a local `key.properties`, are not checked.
Every run prints how many files of each kind it checked; a check that finds no
files where some are required fails instead of passing.

## Checks

| Check | What it proves | What it does not prove |
| --- | --- | --- |
| Required policies | The policy documents exist and are not empty. `PRODUCT_INVARIANTS.md` still lists at least 14 invariants and states the ones CI enforces. `LICENSE` is Apache-2.0. The gate workflows still exist and still run their commands. Every GitHub Action is pinned to a commit SHA. | That the policies are correct, or that branch protection requires the gates. Required status checks are configured in GitHub, not in the repository. |
| Android permissions | No repository manifest outside the `debug` and `profile` source sets requests a forbidden permission (INTERNET, network state, Wi-Fi state, advertising ID), and every other permission is in a reviewed allowlist, which is empty today. | Permissions added by plugins from the pub cache. The **merged-manifest** check below covers those. |
| Merged release manifest (`android.yml`) | The manifest Gradle merges for release, including every plugin, requests no forbidden or unreviewed permission. App-defined signature permissions (such as AndroidX's `DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION`) are accepted. | Native code that opens sockets without the permission (Android blocks this at runtime, which is why the permission check matters). |
| Network capability | No iOS entitlement file grants networking. The app's own Dart (`apps/mobile/lib`), Rust (`core`), Kotlin, Java, Swift and Objective-C code does not use known network APIs, including `Image.network` and `NetworkImage`, which would fetch remote resources. | Network use inside dependencies, or through an API the patterns do not name. iOS has no permission that blocks networking, so on iOS this check and the dependency check are the only automated guard. |
| Dependencies | No package in any lockfile, including development dependencies, is a known ads, analytics, telemetry, crash-reporting, purchase, cloud-AI, web-view or network-client package. Gradle, CocoaPods and SwiftPM files name no such SDK. The original plain-text marker scan still runs. | That an unlisted package is harmless. The lists are a denylist; new dependencies still need the review that `ENGINEERING_STANDARDS.md` requires. |
| Lockfile inventory | Every tracked lockfile is classified: shipped lockfiles (`apps/mobile/pubspec.lock`, `core/Cargo.lock`) are the ones `check_licenses.py` inventories, and any other lockfile is listed as not shipped, with a reason. The inventory file exists and parses. | License correctness. `check_licenses.py`, `cargo deny` and dependency review cover that. |
| Secrets | No tracked file is signing material, a credentials file, or Firebase configuration, and no file contains a private key or a token in a known format (AWS, GitHub, Google, Slack, Stripe, Anthropic, OpenAI). | Arbitrary passwords or keys in an unknown format. Turn on GitHub secret scanning and push protection as well. |

The `debug` and `profile` source sets may request INTERNET because Flutter
needs it for hot reload and the Dart VM service. Neither reaches a release
build, and the merged release manifest check confirms that.

## Changing a check

Product invariant 14 applies: fix a failing check, never delete or loosen it to
make CI pass. Adding a permission to the allowlist, a lockfile to the
not-shipped list, or rewording an invariant the script anchors on is a
reviewed change with the reason written beside it. Every check has a test in
`tests/scripts/test_check_invariants.py` that proves it fails on a violation;
keep one for any new rule.
