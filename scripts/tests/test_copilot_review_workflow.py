from __future__ import annotations

import json
from pathlib import Path
import shutil
import subprocess
import textwrap
import unittest

ROOT = Path(__file__).resolve().parents[2]
RUNNER = r'''
const input = JSON.parse(require('node:fs').readFileSync(0, 'utf8'));
const writes = [], logs = [];
const list = () => {}, listReviews = () => {}, listForRef = () => {};
const github = {
  rest: {
    pulls: {
      list, listReviews,
      get: async () => ({data: input.current || input.pr}),
      requestReviewers: async value => {
        writes.push(['request', value]);
        if (input.requestError) throw new Error('request failed');
      },
    },
    checks: {listForRef},
    issues: {addLabels: async value => {
      if (input.labelError) throw new Error('claim failed');
      writes.push(['claim', value]);
    }},
  },
  paginate: async (method, args) => {
    if (method === list) return [input.pr];
    if (method === listReviews) return input.reviews || [];
    if (method === listForRef) return input.checks;
    if (typeof method === 'string' && method.includes('/rules/branches/')) {
      if (input.policyError) throw new Error('policy unavailable');
      return input.rules;
    }
    throw new Error('unexpected API');
  },
};
const context = {repo: {owner: 'owner', repo: 'project'}, eventName: input.event || 'workflow_run',
  payload: {inputs: {pull_number: input.number || '7'}}};
const core = {info: value => logs.push(value)};
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
new AsyncFunction('github', 'context', 'core', input.script)(github, context, core)
  .then(() => process.stdout.write(JSON.stringify({writes, logs})))
  .catch(error => process.stdout.write(JSON.stringify({writes, logs, error: error.message})));
'''


class CopilotReviewWorkflowTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.node = shutil.which('node')
        if not cls.node:
            raise RuntimeError('Node.js is required for the Copilot workflow contract')
        cls.workflow = (ROOT / '.github/workflows/copilot-review.yml').read_text(encoding='utf-8')
        cls.script = textwrap.dedent(cls.workflow.split('          script: |\n', 1)[1])

    def fixture(self):
        return {
            'pr': {'number': 7, 'state': 'open', 'draft': False,
                   'user': {'login': 'contributor', 'type': 'User'},
                   'head': {'sha': 'current'}, 'base': {'ref': 'main'},
                   'labels': [], 'requested_reviewers': []},
            'rules': [{'type': 'required_status_checks', 'parameters': {
                'required_status_checks': [{'context': 'native', 'integration_id': 15368},
                                           {'context': 'architecture', 'integration_id': 15368}]}}],
            'checks': [{'id': i, 'name': name, 'head_sha': 'current', 'status': 'completed',
                        'conclusion': 'success', 'app': {'id': 15368}}
                       for i, name in enumerate(('native', 'architecture'), 1)],
        }

    def run_gate(self, data):
        result = subprocess.run([self.node, '-e', RUNNER],
                                input=json.dumps({'script': self.script, **data}),
                                text=True, encoding='utf-8', capture_output=True, check=True)
        return json.loads(result.stdout)

    def test_external_ready_pr_claimed_then_requested_after_all_required_ci(self):
        result = self.run_gate(self.fixture())
        self.assertEqual([row[0] for row in result['writes']], ['claim', 'request'])
        self.assertEqual(result['writes'][1][1]['reviewers'], ['copilot-pull-request-reviewer[bot]'])

    def test_owner_draft_closed_and_bot_prs_never_request(self):
        for change in ({'user': {'login': 'OwNeR', 'type': 'User'}}, {'draft': True},
                       {'state': 'closed'}, {'user': {'login': 'dependabot', 'type': 'Bot'}}):
            with self.subTest(change=change):
                data = self.fixture()
                data['pr'].update(change)
                self.assertEqual(self.run_gate(data)['writes'], [])

    def test_failed_pending_skipped_neutral_missing_wrong_app_and_old_checks_defer(self):
        for change in ({'conclusion': 'failure'}, {'status': 'in_progress'},
                       {'conclusion': 'skipped'}, {'conclusion': 'neutral'},
                       {'name': 'unrelated'}, {'app': {'id': 123}}, {'head_sha': 'old'}):
            with self.subTest(change=change):
                data = self.fixture()
                data['checks'][0].update(change)
                self.assertEqual(self.run_gate(data)['writes'], [])

    def test_newer_pending_attempt_cannot_reuse_old_success(self):
        data = self.fixture()
        data['checks'].append({**data['checks'][0], 'id': 99, 'status': 'queued', 'conclusion': None})
        self.assertEqual(self.run_gate(data)['writes'], [])

    def test_previous_review_on_any_commit_prevents_automatic_rereview(self):
        data = self.fixture()
        data['reviews'] = [{'user': {'login': 'copilot-pull-request-reviewer[bot]'}, 'commit_id': 'old'}]
        self.assertEqual(self.run_gate(data)['writes'], [])

    def test_pending_request_or_persistent_claim_prevents_duplicates(self):
        for field, value in (
            ('labels', [{'name': 'copilot-review-requested'}]),
            ('requested_reviewers', [{'login': 'copilot-pull-request-reviewer[bot]'}]),
        ):
            data = self.fixture()
            data['pr'][field] = value
            self.assertEqual(self.run_gate(data)['writes'], [])

    def test_push_draft_close_base_change_or_claim_during_reads_defers(self):
        for change in ({'head': {'sha': 'new'}}, {'draft': True}, {'state': 'closed'},
                       {'base': {'ref': 'release'}}, {'labels': [{'name': 'copilot-review-requested'}]}):
            with self.subTest(change=change):
                data = self.fixture()
                data['current'] = {**data['pr'], **change}
                self.assertEqual(self.run_gate(data)['writes'], [])

    def test_missing_or_unreadable_policy_fails_closed(self):
        data = self.fixture()
        data['rules'] = []
        self.assertEqual(self.run_gate(data)['writes'], [])
        data['policyError'] = True
        self.assertEqual(self.run_gate(data)['error'], 'policy unavailable')

    def test_write_failure_does_not_retry_or_erase_claim(self):
        data = self.fixture()
        data['requestError'] = True
        result = self.run_gate(data)
        self.assertEqual(result['error'], 'request failed')
        self.assertEqual([row[0] for row in result['writes']], ['claim', 'request'])
        data['labelError'] = True
        self.assertEqual(self.run_gate(data)['writes'], [])

    def test_manual_backfill_uses_same_gate_and_validates_number(self):
        data = {**self.fixture(), 'event': 'workflow_dispatch'}
        self.assertEqual(len(self.run_gate(data)['writes']), 2)
        data['number'] = '7; arbitrary code'
        self.assertEqual(self.run_gate(data)['error'], 'Invalid PR number')

    def test_workflow_never_executes_fork_content_or_approves_merges(self):
        self.assertNotIn('actions/checkout', self.workflow)
        self.assertNotIn('secrets.', self.workflow)
        self.assertNotIn('contents: write', self.workflow)
        self.assertNotIn('createReview', self.script)
        self.assertNotIn('synchronize', self.workflow)
        self.assertIn('types: [ready_for_review]', self.workflow)
        self.assertIn("github.event.workflow_run.conclusion == 'success'", self.workflow)
        self.assertIn('retries: 0', self.workflow)
        self.assertIn('cancel-in-progress: false', self.workflow)


if __name__ == '__main__':
    unittest.main()
