import unittest

from conduit_kit.providers import db


class DbKitTest(unittest.TestCase):
    def test_maps_read_request_from_wit(self):
        request = db.read_request_from_wit(
            Record(
                service="consumer-investment",
                resource_name="product",
                environment="test",
                id=None,
                filters=[Record(field="type", value="STOCK")],
                limit=20,
            )
        )

        self.assertEqual(request.service, "consumer-investment")
        self.assertEqual(request.filters[0], db.FieldFilter("type", "STOCK"))

    def test_maps_resource_description_to_wit(self):
        description = db.resource_description_to_wit(
            FakeDbProvider,
            db.ResourceDescription(
                provider="fixture",
                service="consumer-investment",
                resource_name="product",
                environment="test",
                id_field="uid",
                fields=[db.FieldDescription(name="uid", data_type="uuid")],
            ),
        )

        self.assertEqual(description.id_field, "uid")
        self.assertEqual(description.fields[0].data_type, "uuid")

    def test_maps_read_result_to_wit(self):
        result = db.read_result_to_wit(
            FakeDbProvider,
            db.ReadResult(
                status=db.ReadStatus.OK,
                provider="fixture",
                service="consumer-investment",
                resource_name="product",
                environment="test",
                records_json=['{"uid":"1"}'],
            ),
        )

        self.assertIsInstance(result.status, FakeDbProvider.ReadStatus_Ok)
        self.assertEqual(result.shown, 1)


class Record:
    def __init__(self, **values):
        self.__dict__.update(values)


class FakeDbProvider:
    class ReadStatus_Ok:
        pass

    ResourceList = Record
    DbResource = Record
    ResourceDescription = Record
    FieldDescription = Record
    ReadResult = Record


if __name__ == "__main__":
    unittest.main()
