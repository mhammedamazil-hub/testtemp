"""Unit tests for the research agent loop.

These use a fake browser, so they are deterministic and need no network. They
verify the loop, the budgets, the ledger and the policy plumbing — not
Obscura's engine. Live behaviour is covered by ``evals/run_evals.py``.
"""

import unittest

from agent import (Budget, HeuristicSelector, LLMSelector, MissingEvidence,
                   Objective, ResearchAgent, ScriptedSelector, SelectorUnavailable,
                   SourceLedger, Status)
from agent.classify import classify_error, looks_like_challenge
from agent.types import Action, FailureKind, Link


class FakeBrowser:
    """Stands in for BrowserTools. Serves canned observations per URL."""

    def __init__(self, pages, failures=None, verify_finds=None):
        self.pages = pages            # url -> {title, text, links}
        self.failures = failures or {}  # url -> error message
        self.verify_finds = verify_finds if verify_finds is not None else True
        self.calls = []

    def call(self, name, arguments=None):
        arguments = arguments or {}
        self.calls.append((name, arguments))
        url = arguments.get("url", "")
        expression = arguments.get("expression", "")

        if url in self.failures:
            return {"ok": False, "tool": name,
                    "error": {"type": "nonzero_exit", "message": self.failures[url]}}

        page = self.pages.get(url)
        if page is None:
            return {"ok": False, "tool": name,
                    "error": {"type": "nonzero_exit",
                              "message": f"dns error: no such host for {url}"}}

        # Verification query: the agent asks whether body text contains a needle.
        if "occurrences" in expression:
            return {"ok": True, "tool": name,
                    "result": '{"found": %s, "occurrences": %d}'
                              % (str(self.verify_finds).lower(),
                                 1 if self.verify_finds else 0)}

        import json
        payload = {"title": page.get("title", ""),
                   "text": page.get("text", ""),
                   "links": page.get("links", [])}
        return {"ok": True, "tool": name, "result": json.dumps(payload)}


PAGE_A = {
    "title": "Project A",
    "text": "Welcome to project A.\nSee the releases page for versions.",
    "links": [
        {"href": "https://site.test/releases", "text": "Releases"},
        {"href": "https://site.test/about", "text": "About"},
    ],
}
PAGE_B = {
    "title": "Releases · Project A",
    "text": "Latest release is v1.4.2, published in May.\nOlder: v1.4.1",
    "links": [],
}


def obj(**overrides):
    base = dict(question="What is the latest release?",
                start_url="https://site.test/",
                answer_pattern=r"(v\d+\.\d+\.\d+)",
                link_keywords=["releases"],
                max_hops=1)
    base.update(overrides)
    return Objective(**base)


class TestLoop(unittest.TestCase):
    def test_follows_link_and_answers(self):
        browser = FakeBrowser({"https://site.test/": PAGE_A,
                               "https://site.test/releases": PAGE_B})
        result = ResearchAgent(browser, budget=Budget(max_steps=6)).run(obj())

        self.assertEqual(result.status, Status.COMPLETE, result.summary)
        self.assertEqual(result.answer, "v1.4.2")
        # Ran observe -> follow -> finish, not a hardcoded script.
        self.assertGreaterEqual(len(result.observations), 2)
        self.assertEqual(result.observations[1].url, "https://site.test/releases")

    def test_claim_is_cited_and_verified(self):
        browser = FakeBrowser({"https://site.test/": PAGE_A,
                               "https://site.test/releases": PAGE_B})
        result = ResearchAgent(browser, budget=Budget(max_steps=6)).run(obj())

        claims = result.claims.claims
        self.assertEqual(len(claims), 1)
        claim = claims[0]
        self.assertEqual(claim.url, "https://site.test/releases")
        self.assertTrue(claim.verified)
        self.assertIn("v1.4.2", claim.evidence)

    def test_unverified_claim_is_flagged(self):
        browser = FakeBrowser({"https://site.test/": PAGE_A,
                               "https://site.test/releases": PAGE_B},
                              verify_finds=False)
        result = ResearchAgent(browser, budget=Budget(max_steps=6)).run(obj())
        claim = result.claims.claims[0]
        self.assertFalse(claim.verified)

    def test_answers_without_following_when_present_on_start_page(self):
        page = {"title": "Home", "text": "Version v9.9.9 is live.",
                "links": [{"href": "https://site.test/releases", "text": "Releases"}]}
        browser = FakeBrowser({"https://site.test/": page})
        result = ResearchAgent(browser, budget=Budget(max_steps=6)).run(obj())
        self.assertEqual(result.answer, "v9.9.9")
        self.assertEqual(len(result.observations), 1, "should not hop needlessly")

    def test_missing_answer_reports_incomplete(self):
        page = {"title": "Nothing here", "text": "No versions listed.", "links": []}
        browser = FakeBrowser({"https://site.test/": page})
        result = ResearchAgent(browser, budget=Budget(max_steps=6)).run(obj())
        self.assertEqual(result.status, Status.INCOMPLETE)
        self.assertIsNone(result.answer)
        self.assertEqual(result.claims.claims, [])

    def test_absent_fact_is_a_correct_outcome(self):
        page = {"title": "Nothing here", "text": "No versions listed.", "links": []}
        browser = FakeBrowser({"https://site.test/": page})
        result = ResearchAgent(browser, budget=Budget(max_steps=6)).run(
            obj(expects_answer=False))
        # Distinct from COMPLETE: no answer exists, and saying so is correct.
        self.assertEqual(result.status, Status.NOT_FOUND)
        self.assertEqual(result.claims.claims, [])

    def test_link_index_skips_filtered_links(self):
        """Regression: indices must match the filtered list the agent uses.

        The first two anchors are non-http and are dropped; the Releases link
        is therefore at index 0 of the kept list. If indices were assigned
        before filtering, the agent would navigate to the wrong URL.
        """
        page = {
            "title": "Project", "text": "See releases.",
            "links": [
                {"href": "mailto:team@site.test", "text": "Releases"},
                {"href": "javascript:void(0)", "text": "Releases"},
                {"href": "https://site.test/releases", "text": "Releases"},
            ],
        }
        browser = FakeBrowser({"https://site.test/": page,
                               "https://site.test/releases": PAGE_B})
        result = ResearchAgent(browser, budget=Budget(max_steps=6)).run(obj())
        self.assertEqual(result.observations[-1].url, "https://site.test/releases",
                         "agent navigated to the wrong link")
        self.assertEqual(result.answer, "v1.4.2")

    def test_same_page_anchor_is_not_followed(self):
        """A '#section' link points at the current document; following it is a no-op."""
        page = {
            "title": "Releases", "text": "Nothing matching here.",
            "links": [
                {"href": "https://site.test/#releases", "text": "Releases"},
                {"href": "https://site.test/", "text": "Releases"},
            ],
        }
        browser = FakeBrowser({"https://site.test/": page})
        result = ResearchAgent(browser, budget=Budget(max_steps=6)).run(
            obj(answer_pattern=r"(zzz\d+)", max_hops=2))
        visited = [o.url for o in result.observations]
        self.assertEqual(visited, ["https://site.test/"],
                         f"should not re-fetch the same document: {visited}")


class TestFailureSeparation(unittest.TestCase):
    def test_network_block_is_not_a_browser_error(self):
        browser = FakeBrowser({}, failures={
            "https://site.test/":
                "Error: ... client error (Connect): tls handshake eof"})
        result = ResearchAgent(browser, budget=Budget(max_steps=4)).run(obj())
        self.assertEqual(result.status, Status.FAILED)
        self.assertEqual(result.error.kind, FailureKind.NETWORK_BLOCKED)

    def test_dns_failure_is_distinct(self):
        browser = FakeBrowser({})  # unknown host path returns a dns error
        result = ResearchAgent(browser, budget=Budget(max_steps=4)).run(obj())
        self.assertEqual(result.error.kind, FailureKind.DNS_FAILURE)

    def test_bot_challenge_detected_from_title(self):
        page = {"title": "Just a moment...", "text": "Checking your browser",
                "links": []}
        browser = FakeBrowser({"https://site.test/": page})
        result = ResearchAgent(browser, budget=Budget(max_steps=4)).run(obj())
        self.assertEqual(result.error.kind, FailureKind.BOT_CHALLENGE)
        self.assertEqual(result.claims.claims, [], "a challenge page is not evidence")

    def test_classifier_categories(self):
        cases = {
            "tls handshake eof": FailureKind.NETWORK_BLOCKED,
            "SSL_ERROR_SYSCALL": FailureKind.NETWORK_BLOCKED,
            "dns error: failed to lookup address information": FailureKind.DNS_FAILURE,
            "invalid peer certificate: UnknownIssuer": FailureKind.TLS_TRUST,
            "operation timed out": FailureKind.TIMEOUT,
            "HTTP 403 cf-mitigated: challenge": FailureKind.BOT_CHALLENGE,
            "something odd happened": FailureKind.BROWSER_ERROR,
        }
        for message, expected in cases.items():
            with self.subTest(message=message):
                self.assertEqual(classify_error(message).kind, expected)

    def test_challenge_titles(self):
        self.assertTrue(looks_like_challenge("Just a moment...", ""))
        self.assertTrue(looks_like_challenge("Attention Required! | Cloudflare", ""))
        self.assertFalse(looks_like_challenge("Releases · project", "v1.0.0"))


class TestBudgets(unittest.TestCase):
    def test_step_limit_stops_the_run(self):
        browser = FakeBrowser({"https://site.test/": PAGE_A,
                               "https://site.test/releases": PAGE_B})
        result = ResearchAgent(browser, budget=Budget(max_steps=1, max_pages=1)).run(obj())
        self.assertEqual(result.status, Status.LIMIT_REACHED)
        self.assertLessEqual(result.budget.steps_used, 1)

    def test_page_limit_stops_the_run(self):
        browser = FakeBrowser({"https://site.test/": PAGE_A,
                               "https://site.test/releases": PAGE_B})
        result = ResearchAgent(browser, budget=Budget(max_steps=10, max_pages=1)).run(obj())
        self.assertEqual(result.status, Status.LIMIT_REACHED)

    def test_time_limit(self):
        browser = FakeBrowser({"https://site.test/": PAGE_A})
        result = ResearchAgent(browser, budget=Budget(max_seconds=0.0)).run(obj())
        self.assertEqual(result.status, Status.LIMIT_REACHED)

    def test_retries_are_bounded(self):
        browser = FakeBrowser({}, failures={
            "https://site.test/": "operation timed out"})
        result = ResearchAgent(browser,
                               budget=Budget(max_steps=3, max_pages=5,
                                             max_retries_per_action=2,
                                             retry_backoff_s=0.0)).run(obj())
        self.assertEqual(result.error.kind, FailureKind.TIMEOUT)
        # 1 initial attempt + 2 retries
        self.assertLessEqual(result.budget.pages_used, 3)


class TestLedger(unittest.TestCase):
    def test_ledger_refuses_claims_without_evidence(self):
        ledger = SourceLedger()
        with self.assertRaises(MissingEvidence):
            ledger.add(text="v1.0.0", url="https://x.test", evidence="", method="m", step=1)
        with self.assertRaises(MissingEvidence):
            ledger.add(text="v1.0.0", url="", evidence="v1.0.0", method="m", step=1)

    def test_ledger_deduplicates(self):
        ledger = SourceLedger()
        ledger.add("v1", "https://x.test", "evidence", "m", 1)
        ledger.add("v1", "https://x.test", "evidence", "m", 2)
        self.assertEqual(len(ledger.claims), 1)

    def test_markdown_rendering(self):
        ledger = SourceLedger()
        ledger.add("v1", "https://x.test", "ev", "m", 1)
        self.assertIn("https://x.test", ledger.render_markdown())


class TestPolicies(unittest.TestCase):
    def test_llm_selector_is_unavailable_without_a_model(self):
        selector = LLMSelector()
        self.assertFalse(selector.available())
        with self.assertRaises(SelectorUnavailable):
            selector.choose(obj(), [], None)

    def test_llm_selector_parses_a_reply(self):
        calls = []

        def complete(prompt):
            calls.append(prompt)
            return '{"action": "finish", "claim": "v1.2.3", "evidence": "v1.2.3"}'

        selector = LLMSelector(complete=complete, model_label="stub")
        self.assertTrue(selector.available())
        action = selector.choose(obj(), [], None)
        self.assertEqual(action.kind, "finish")
        self.assertEqual(action.claim, "v1.2.3")
        self.assertIn("objective", calls[0].lower())

    def test_llm_selector_rejects_bad_json(self):
        selector = LLMSelector(complete=lambda prompt: "not json")
        with self.assertRaises(SelectorUnavailable):
            selector.choose(obj(), [], None)

    def test_llm_selector_rejects_unknown_action(self):
        selector = LLMSelector(complete=lambda prompt: '{"action": "delete_everything"}')
        with self.assertRaises(SelectorUnavailable):
            selector.choose(obj(), [], None)

    def test_eval_action_executes_javascript(self):
        """A policy can ask for a JS expression and inspect the result."""
        page = {"title": "Project", "text": "no version here", "links": []}
        browser = FakeBrowser({"https://site.test/": page})
        script = ScriptedSelector([
            Action(kind="navigate", url="https://site.test/"),
            Action(kind="eval", expression="document.title"),
            Action(kind="finish", summary="done"),
        ])
        result = ResearchAgent(browser, selector=script,
                               budget=Budget(max_steps=6)).run(obj())
        eval_steps = [o for o in result.observations if o.action == "eval"]
        self.assertEqual(len(eval_steps), 1)
        # FakeBrowser returns the whole page payload for non-verification queries.
        self.assertIsNotNone(eval_steps[0].js_result)

    def test_eval_before_navigation_is_an_error_not_a_crash(self):
        script = ScriptedSelector([Action(kind="eval", expression="1+1")])
        result = ResearchAgent(FakeBrowser({}), selector=script,
                               budget=Budget(max_steps=3)).run(obj())
        self.assertEqual(result.status, Status.FAILED)
        self.assertEqual(result.error.kind, FailureKind.BROWSER_ERROR)

    def test_scripted_selector_drives_the_loop(self):
        browser = FakeBrowser({"https://site.test/": PAGE_A,
                               "https://site.test/releases": PAGE_B})
        script = ScriptedSelector([
            Action(kind="navigate", url="https://site.test/"),
            Action(kind="follow", link_index=0),
            Action(kind="finish", claim="v1.4.2", evidence="v1.4.2"),
        ])
        result = ResearchAgent(browser, selector=script,
                               budget=Budget(max_steps=6)).run(obj())
        self.assertEqual(result.status, Status.COMPLETE)
        self.assertEqual(result.answer, "v1.4.2")

    def test_llm_run_reports_not_connected(self):
        browser = FakeBrowser({"https://site.test/": PAGE_A})
        result = ResearchAgent(browser, budget=Budget(max_steps=3)).run(obj())
        self.assertFalse(result.llm_connected)
        self.assertEqual(result.selector, "heuristic")


if __name__ == "__main__":
    unittest.main()
