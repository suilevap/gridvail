// Decides what a web build run publishes, from the event that started it.
// Previews use the branch's own commit, never GitHub's trial merge with
// main, so a pull request with merge conflicts still gets its preview.
//
// Outputs:
//   dest    "." (site root, main), "pr/<n>" (a preview), or "" (nothing)
//   sha     the commit to build
//   pr      the pull request number, for previews
//   remove  "true" when a closed pull request's preview should go
const PREVIEW_OPT_OUT = 'no-preview';

module.exports = async ({ context, core }) => {
  const { owner, repo } = context.repo;
  const set = ({ dest = '', sha = '', pr = '', remove = false }) => {
    core.setOutput('dest', dest);
    core.setOutput('sha', sha);
    core.setOutput('pr', String(pr));
    core.setOutput('remove', String(remove));
    core.notice(remove ? `Remove pr/${pr}` : dest ? `Publish ${dest} from ${sha.slice(0, 7)}` : 'Nothing to publish');
  };

  if (context.eventName === 'pull_request_target') {
    const pr = context.payload.pull_request;
    // Fork branches are not ours to build with a write token around.
    if (pr.head.repo?.full_name !== `${owner}/${repo}`) return set({});
    if (context.payload.action === 'closed') return set({ pr: pr.number, remove: true });
    if (pr.labels.some((label) => label.name === PREVIEW_OPT_OUT)) return set({});
    return set({ dest: `pr/${pr.number}`, sha: pr.head.sha, pr: pr.number });
  }

  // A push or a manual run publishes the site root, from main only.
  if (context.ref === 'refs/heads/main') return set({ dest: '.', sha: context.sha });
  return set({});
};
