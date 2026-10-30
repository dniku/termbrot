#!/usr/bin/env bash
# One-shot acceptance run. Requires network (nixpkgs input) and write access to
# .git, both of which the sandbox denies -- run it in a normal shell.
#
# Flakes build the COMMITTED tree, so the commit here is not bookkeeping: without
# it, nix keeps rebuilding whichever snapshot was committed last.
set -euo pipefail

if [[ -n "$(git status --porcelain)" ]]; then
  echo "==> committing working tree so the flake can see it"
  git add -A
  git -c user.email="${EMAIL:-dev@example.com}" -c user.name="${NAME:-dev}" \
    commit -m "png: big-endian Adler-32; tests and docs for the fixed-point scale"
else
  echo "==> working tree clean, building as committed"
fi

# doCheck is enabled in mandelbrot.nix, so this build *is* the unit test run:
# a test failure fails the build.
echo "==> nix build (runs the unit tests)"
nix build

echo "==> nix flake check"
nix flake check

echo "==> render smoke test (terminal)"
./result/bin/mandelbrot -w 60 -h 24 -i 200

echo "==> PNG conformance (needs a real decoder)"
./result/bin/mandelbrot -w 200 -h 120 -o /tmp/codex/frame.png
file /tmp/codex/frame.png

echo "==> zoomed render"
./result/bin/mandelbrot -c -0.75,-0.1 -z 40 -w 80 -h 40

echo "All checks passed."
