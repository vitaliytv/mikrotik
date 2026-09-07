import base64
import hashlib
import importlib.util
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).parents[1] / "modem_radio_collector.py"
SPEC = importlib.util.spec_from_file_location("modem_radio_collector", MODULE_PATH)
collector = importlib.util.module_from_spec(SPEC)
assert SPEC.loader
SPEC.loader.exec_module(collector)


class CollectorTests(unittest.TestCase):
    def test_zte_password_hash_matches_device_algorithm(self):
        expected = hashlib.sha256(f"{hashlib.sha256(b'secret').hexdigest()}digest".encode()).hexdigest()
        self.assertEqual(collector.zte_password_hash("secret", "digest"), expected)

    def test_huawei_password_hash_matches_password_type_four(self):
        inner = base64.b64encode(hashlib.sha256(b"secret").digest()).decode()
        expected = base64.b64encode(hashlib.sha256(f"admin{inner}token".encode()).digest()).decode()
        self.assertEqual(collector.huawei_password_hash("admin", "secret", "token"), expected)

    def test_xml_and_numeric_metric_parsing(self):
        self.assertEqual(
            collector.xml_values("<response><rsrp>-97dBm</rsrp><pci>123</pci></response>"),
            {"rsrp": "-97dBm", "pci": "123"},
        )
        self.assertEqual(collector.metric("-12.5dB"), "-12.5")
        self.assertEqual(collector.metric(""), "")

    def test_radio_log_is_machine_readable_and_sanitized(self):
        line = collector.radio_log_line(
            "bite",
            "up",
            {"operator": "Bite LV", "network": "LTE = CA", "rsrp_dbm": "-97"},
        )
        self.assertEqual(
            line,
            "WANQUALITY type=radio wan=bite status=up operator=Bite_LV "
            "network=LTE_CA rsrp_dbm=-97",
        )
        self.assertNotIn("secret", line)


if __name__ == "__main__":
    unittest.main()
