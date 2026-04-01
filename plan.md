# DARI cleanup roadmap

## Step 19. Move generation of completions to separate subcommand

Introduce `dari completions <SHELL>` subcommand, that will write completion script to stdout. Remove this logic from `build.rs`

## Step 20. Encrypt command improvments

1. Change logic to save file from in-place by default to `<basename>.enc.dar`
2. Add `-o` argument to specify custom filename to save encrypted archive as
3. Add `-i` argument to save in-place
