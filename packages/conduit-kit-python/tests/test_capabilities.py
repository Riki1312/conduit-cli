import unittest

from conduit_kit.capabilities.http import Header, HttpClient, normalize_headers
from conduit_kit.testing.http import FakeHttpClient, response
from conduit_kit.testing.secrets import FakeSecretStore


class CapabilityKitTest(unittest.TestCase):
    def test_http_client_maps_requests_to_generated_binding(self):
        binding = FakeHttpBinding()
        client = HttpClient(binding)

        result = client.post(
            "https://logs.example.com/search",
            headers={"content-type": "application/json"},
            body="{}",
            timeout_ms=1000,
        )

        self.assertEqual(result.status, 202)
        self.assertIsInstance(binding.requests[0].method, FakeHttpBinding.HttpMethod_Post)
        self.assertEqual(binding.requests[0].headers[0].name, "content-type")
        self.assertEqual(binding.requests[0].body, "{}")

    def test_fake_http_records_requests(self):
        client = FakeHttpClient([response(200, "ok")])

        result = client.post(
            "https://logs.example.com/search",
            headers={"content-type": "application/json"},
            body="{}",
            timeout_ms=1000,
        )

        self.assertEqual(result.status, 200)
        self.assertEqual(client.requests[0]["method"], "post")
        self.assertEqual(client.requests[0]["headers"][0].name, "content-type")

    def test_fake_secret_store_behaves_like_user_scoped_store(self):
        store = FakeSecretStore()

        self.assertIsNone(store.read("company/staging/token"))
        self.assertTrue(store.write("company/staging/token", "secret"))
        self.assertEqual(store.read("company/staging/token"), "secret")
        self.assertTrue(store.delete("company/staging/token"))
        self.assertIsNone(store.read("company/staging/token"))

    def test_normalizes_mapping_and_sequence_headers(self):
        self.assertEqual(
            normalize_headers({"a": "b"}),
            [Header(name="a", value="b")],
        )
        self.assertEqual(
            normalize_headers([Header(name="a", value="b")]),
            [Header(name="a", value="b")],
        )


class FakeHttpBinding:
    class HttpMethod_Get:
        pass

    class HttpMethod_Post:
        pass

    class HttpHeader:
        def __init__(self, **values):
            self.__dict__.update(values)

    class HttpRequest:
        def __init__(self, **values):
            self.__dict__.update(values)

    class HttpResponse:
        def __init__(self, **values):
            self.__dict__.update(values)

    def __init__(self):
        self.requests = []

    def request(self, request):
        self.requests.append(request)
        return self.HttpResponse(status=202, headers=[], body="accepted")


if __name__ == "__main__":
    unittest.main()
