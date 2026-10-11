// Keeps one web-preview comment per pull request, found by a hidden marker:
// updated in place instead of adding a comment on every push.
const MARKER = '<!-- web-preview -->';

module.exports = async ({ github, context }, issue_number, body) => {
  const { owner, repo } = context.repo;
  const comments = await github.paginate(github.rest.issues.listComments, {
    owner,
    repo,
    issue_number,
  });
  const existing = comments.find((comment) => comment.body?.includes(MARKER));
  const text = `${MARKER}\n${body}`;
  if (existing) {
    await github.rest.issues.updateComment({ owner, repo, comment_id: existing.id, body: text });
  } else {
    await github.rest.issues.createComment({ owner, repo, issue_number, body: text });
  }
};
