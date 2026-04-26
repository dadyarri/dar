# v5 Ignore Rules

Recursive directory scans respect:

- `.gitignore`
- `.darignore`

Hidden files are included unless excluded by one of those rule sources.

Individual file paths passed directly on the command line are added as explicit inputs
instead of being filtered through recursive ignore matching.
