# DARI cleanup roadmap

## Step 19. Move generation of completions to separate subcommand

Introduce `dari completions <SHELL>` subcommand, that will write completion script to stdout. Remove this logic from `build.rs`
