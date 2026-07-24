import unittest

from conduit_kit.providers import logs


class LogsKitTest(unittest.TestCase):
    def test_maps_query_from_wit(self):
        query = logs.query_from_wit(
            Record(
                service="checkout",
                environment="staging",
                time_range=Record(
                    from_="2026-07-24T10:00:00Z",
                    to="2026-07-24T10:15:00Z",
                    source="since 15m",
                ),
                limit=20,
                levels=["ERROR"],
                cid="CID-123",
                trace_id=None,
                message="failed",
                grep="stack trace",
                logger="CheckoutService",
                exclude_messages=["noise"],
                exclude_greps=["known trace"],
                exclude_loggers=["NoisyLogger"],
                include_trace=True,
                cursor=None,
            )
        )

        self.assertEqual(query.service, "checkout")
        self.assertEqual(query.time_range.source, "since 15m")
        self.assertEqual(query.grep, "stack trace")
        self.assertEqual(query.exclude_greps, ["known trace"])

    def test_maps_search_result_to_wit(self):
        result = logs.search_result_to_wit(
            FakeLogsProvider,
            logs.SearchResult(
                status=logs.Status.OK,
                provider="fixture",
                service="checkout",
                environment="staging",
                time_range=logs.TimeRange(
                    from_="2026-07-24T10:00:00Z",
                    to="2026-07-24T10:15:00Z",
                    source="since 15m",
                ),
                matches=2,
                logs=[
                    logs.Event(
                        id="1",
                        timestamp="2026-07-24T10:01:00Z",
                        level="ERROR",
                        service="checkout",
                        environment="staging",
                        cid="CID-123",
                        trace_id=None,
                        logger="CheckoutService",
                        message="failed",
                        stack_trace="RuntimeError",
                    )
                ],
                diagnostics=[logs.Diagnostic(kind="query_truncated", hint="shown 1 of 2")],
            ),
        )

        self.assertIsInstance(result.status, FakeLogsProvider.LogStatus_Ok)
        self.assertEqual(result.shown, 1)
        self.assertEqual(result.logs[0].stack_trace, "RuntimeError")
        self.assertEqual(result.diagnostics[0].kind, "query_truncated")


class Record:
    def __init__(self, **values):
        self.__dict__.update(values)


class FakeLogsProvider:
    class LogStatus_Ok:
        pass

    class AuthStatus_Ok:
        pass

    TimeRange = Record
    LogEvent = Record
    Diagnostic = Record
    SearchResult = Record
    AuthResult = Record


if __name__ == "__main__":
    unittest.main()
