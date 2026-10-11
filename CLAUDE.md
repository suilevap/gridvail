# Notes for agents

See `README.md` for what the game is, how to run it and the agent runtime
API, and `ARCHITECTURE.md` for how the code is laid out.

## Web build and pull request previews

The game runs in the browser. `.github/workflows/pages.yml` builds it to
WebAssembly and publishes it to GitHub Pages from the `gh-pages` branch:

- `main` is served at https://suilevap.github.io/gridvail/
- each pull request from this repository gets a preview at
  `https://suilevap.github.io/gridvail/pr/<number>/`. The workflow posts the
  link in a comment on the pull request, rebuilds it on every push to the
  branch (about 7–10 minutes) and removes it when the pull request closes.

A preview is built from the branch's own commit, not merged with `main`, so
it shows exactly what the branch does, and a pull request with merge
conflicts still gets one. Previews run `main`'s copy of the workflow
(`pull_request_target`), so a pull request that changes
`.github/workflows/pages.yml` or `.github/scripts/` only affects previews once
it is merged.

The page's bottom-right corner names the build it serves (`PR #<n> · <sha>`
for a preview), so a reviewer can tell whether the latest push is live yet.

### Link the preview when the change can be seen or played

When a pull request changes anything a player could notice in the browser
(gameplay, AI, maps, rendering, camera, input, HUD, the web page itself):

- put the preview link in the pull request description; the URL is known as
  soon as the pull request number is, before the build finishes;
- point it at what changed with URL options (the CLI options without `--`,
  listed in `README.md`), e.g. `.../pr/42/?map=portals&reveal` for a portal
  change, and say what to try ("walk through the turning portal at the top");
- include the same link when reporting the finished work to the user.

Touch controls (`src/touch/`) can only be tried on a phone or tablet, so say
so when the change is about them.

### Skip the preview when nothing visible changed

- Pull requests that only touch `**.md`, `tests/**`, `tools/**` or
  `.claude/**` build no preview at all.
- For other changes nobody needs to play (CI, refactors with no visible
  effect, native-only options), add the `no-preview` label when opening the
  pull request; the build is skipped while it is set. A later push rebuilds
  once the label is removed.
- Do not link a preview for a pull request that has none.
