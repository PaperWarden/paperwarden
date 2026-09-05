# Engineering Standards

## Change discipline

- Every change has a focused issue and acceptance criteria.
- Architecture changes require an ADR before implementation.
- New dependencies require purpose, maintenance, license, native-code, network,
  vulnerability, binary-size, and platform-support review.
- Bugs in parsing or lifecycle behavior add a permanent regression fixture/test.
- No one pushes directly to `main` or merges their own pull request.

## Verification

- Formatters, analyzers, compiler warnings, and relevant tests must pass.
- Malformed input, cancellation, failure, and cleanup paths are tested.
- Conversion outputs are structurally or visually verified.
- Redaction tests attempt extraction and search after transformation.
- CI actions and important build dependencies are pinned before release.

## Commits

Use focused conventional commits, for example:

```text
feat(viewer): add PDF outline navigation
security(archive): block parent path traversal
test(redaction): verify removed text cannot be extracted
docs(architecture): record isolated processing model
```

