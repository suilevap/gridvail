// Decides what a web build run publishes, from the event that started it.
// Builds always use a branch's own commit, never GitHub's trial merge with
// main, so a pull request with merge conflicts still gets its preview.
//
// Outputs:
//   dest    "." (site root, main), "pr/<n>" (a preview), or "" (nothing)
//   sha     the commit to build
//   pr      the pull request number, for previews
//   remove  "true" when a closed pull request's preview should go
const PREVIEW_OPT_OUT = 'no-preview';

module.exports = async ({ github, context, core }) => {
  const { owner, repo } = context.repo;
  const set = ({ dest = '', sha = '', pr = '', remove = false }) => {
    core.setOutput('dest', dest);
    core.setOutput('sha', sha);
    core.setOutput('pr', String(pr));
    core.setOutput('remove', String(remove));
    core.notice(remove ? `Remove pr/${pr}` : dest ? `Publish ${dest} from ${sha.slice(0, 7)}` : 'Nothing to publish');
  };
  const optedOut = (labels) => labels.some((label) => label.name === PREVIEW_OPT_OUT);

  if (context.eventName === 'pull_request_target') {
    const pr = context.payload.pull_request;
    // Fork branches are not ours to build with a write token around.
    if (pr.head.repo?.full_name !== `${owner}/${repo}`) return set({});
    if (context.payload.action === 'closed') return set({ pr: pr.number, remove: true });
    if (optedOut(pr.labels)) return set({});
    return set({ dest: `pr/${pr.number}`, sha: pr.head.sha, pr: pr.number });
  }

  // A push or a manual run: main publishes the site root; any other branch
  // publishes the preview of its open pull request, if it has one.
  const branch = context.ref.replace(/^refs\/heads\//, '');
  if (branch === context.payload.repository?.default_branch || branch === 'main') {
    return set({ dest: '.', sha: context.sha });
  }
  const { data: pulls } = await github.rest.pulls.list({
    owner,
    repo,
    state: 'open',
    head: `${owner}:${branch}`,
  });
  const pr = pulls.find((pull) => pull.head.repo?.full_name === `${owner}/${repo}`);
  if (!pr || optedOut(pr.labels)) return set({});
  return set({ dest: `pr/${pr.number}`, sha: context.sha, pr: pr.number });
};
