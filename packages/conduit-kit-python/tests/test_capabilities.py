import unittest

from conduit_kit.capabilities.files import FileReader
from conduit_kit.capabilities.http import Header, HttpClient, normalize_headers
from conduit_kit.errors import CapabilityError
from conduit_kit.testing.files import FakeFileReader
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

    def test_http_client_maps_simple_get_binding(self):
        binding = FakeHttpGetBinding()
        client = HttpClient(binding)

        result = client.get("https://docs.example.test/spec.json")

        self.assertEqual(result.status, 200)
        self.assertEqual(result.headers, [])
        self.assertEqual(result.body, "{}")
        self.assertEqual(binding.urls, ["https://docs.example.test/spec.json"])

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

    def test_file_reader_maps_optional_not_found(self):
        binding = FakeFileReadBinding()
        reader = FileReader(binding)

        self.assertEqual(reader.read_optional_text("missing.json"), None)

    def test_file_reader_maps_errors(self):
        binding = FakeFileReadBinding(kind=object(), message="denied")
        reader = FileReader(binding)

        with self.assertRaisesRegex(CapabilityError, "denied"):
            reader.read_text("blocked.json")

    def test_fake_file_reader_behaves_like_project_files(self):
        reader = FakeFileReader({"spec.json": "{}"})

        self.assertEqual(reader.read_text("spec.json"), "{}")
        self.assertEqual(reader.read_optional_text("missing.json"), None)


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


class FakeHttpGetBinding:
    class HttpResponse:
        def __init__(self, **values):
            self.__dict__.update(values)

    def __init__(self):
        self.urls = []

    def get(self, url):
        self.urls.append(url)
        return self.HttpResponse(status=200, body="{}")


class FakeFileReadBinding:
    class FileReadErrorKind_NotFound:
        pass

    def __init__(self, kind=None, message="missing"):
        self.kind = kind if kind is not None else self.FileReadErrorKind_NotFound()
        self.message = message

    def read_text(self, path):
        raise FakeBindingError(Record(kind=self.kind, message=self.message))


class FakeBindingError(Exception):
    def __init__(self, value):
        self.value = value


class Record:
    def __init__(self, **values):
        self.__dict__.update(values)


if __name__ == "__main__":
    unittest.main()
