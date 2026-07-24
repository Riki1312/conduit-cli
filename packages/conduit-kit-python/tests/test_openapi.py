import unittest

from conduit_kit.providers import openapi


class OpenApiKitTest(unittest.TestCase):
    def test_maps_operation_request_from_wit(self):
        request = openapi.operation_request_from_wit(
            Record(
                service="consumer-wealth",
                environment="staging",
                method="GET",
                path="/products/{id}",
            )
        )

        self.assertEqual(request.service, "consumer-wealth")
        self.assertEqual(request.method, "GET")

    def test_maps_operation_to_wit(self):
        operation = openapi.operation_to_wit(
            FakeOpenApiProvider,
            openapi.Operation(
                service="consumer-wealth",
                environment="staging",
                method="GET",
                path="/products/{id}",
                parameters=[
                    openapi.Parameter(
                        name="id",
                        location=openapi.ParameterLocation.PATH,
                        required=True,
                        schema_json='{"type":"string"}',
                    )
                ],
                operation_id="getProduct",
                response_schema_json='{"type":"object"}',
            ),
        )

        self.assertEqual(operation.service, "consumer-wealth")
        self.assertIsInstance(
            operation.parameters[0].location,
            FakeOpenApiProvider.ParameterLocation_Path,
        )
        self.assertEqual(operation.response_schema_json, '{"type":"object"}')


class Record:
    def __init__(self, **values):
        self.__dict__.update(values)


class FakeOpenApiProvider:
    class ParameterLocation_Path:
        pass

    class ParameterLocation_Query:
        pass

    class ParameterLocation_Header:
        pass

    class ParameterLocation_Cookie:
        pass

    Operation = Record
    Parameter = Record


if __name__ == "__main__":
    unittest.main()
