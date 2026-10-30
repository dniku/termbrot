Acceptance checklist. Run these from a normal shell (not the agent sandbox),
which has network for the nixpkgs input and a working linker.

Flakes build the **committed** tree, so `git status` must be clean before a Nix
build reflects your edits. `cargo check` reads the working tree, so the two
disagree confusingly when something is uncommitted.

```console
$ git status --short          # must be empty
$ nix build                   # runs the unit tests via doCheck
$ nix flake check
$ nix fmt                     # nixfmt over the .nix files
$ cargo clippy --all-targets -- -D warnings
$ cargo fmt --check
```

Render smoke tests:

```console
$ ./result/bin/termbrot -w 80 -h 30            # whole set, symmetric
$ ./result/bin/termbrot -c -0.745,0.113 -z 60  # seahorse valley, real detail
$ ./result/bin/termbrot -w 400 -h 280 -o frame.png
$ file frame.png                                 # expect: PNG image data, ... RGB
```
