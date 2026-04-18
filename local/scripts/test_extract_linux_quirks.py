#!/usr/bin/env python3
import importlib.util
import pathlib
import unittest


SCRIPT_PATH = pathlib.Path(__file__).with_name("extract-linux-quirks.py")
SPEC = importlib.util.spec_from_file_location("extract_linux_quirks", SCRIPT_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"failed to load module spec for {SCRIPT_PATH}")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class ExtractLinuxQuirksPciTests(unittest.TestCase):
    def test_direct_flag_assignment_maps_to_redbear_flag(self):
        source = """
static void quirk_no_d3(struct pci_dev *dev)
{
    dev->dev_flags |= PCI_DEV_FLAGS_NO_D3;
}

DECLARE_PCI_FIXUP_FINAL(0x1002, 0x67DF, quirk_no_d3);
"""

        mapped = MODULE.map_pci_fixups_to_flags(source)

        self.assertEqual(mapped[0][1], ["no_d3cold"])

    def test_multi_flag_assignment_maps_all_supported_flags(self):
        source = """
static void quirk_multi(struct pci_dev *dev)
{
    dev->dev_flags |= PCI_DEV_FLAGS_NO_MSI | PCI_DEV_FLAGS_NO_MSIX | PCI_DEV_FLAGS_NO_ASPM;
}

DECLARE_PCI_FIXUP_HEADER(0x8086, 0x1234, quirk_multi);
"""

        mapped = MODULE.map_pci_fixups_to_flags(source)

        self.assertEqual(mapped[0][1], ["no_aspm", "no_msi", "no_msix"])

    def test_helper_call_maps_no_d3cold(self):
        source = """
static void quirk_disable_d3cold(struct pci_dev *pdev)
{
    pci_d3cold_disable(pdev);
}

DECLARE_PCI_FIXUP_ENABLE(0x1022, 0x1481, quirk_disable_d3cold);
"""

        mapped = MODULE.map_pci_fixups_to_flags(source)

        self.assertEqual(mapped[0][1], ["no_d3cold"])

    def test_unsupported_handler_body_yields_no_flags(self):
        source = """
static void quirk_vendor_workaround(struct pci_dev *dev)
{
    pci_info(dev, "workaround only\n");
}

DECLARE_PCI_FIXUP_LATE(0x10DE, 0x1C82, quirk_vendor_workaround);
"""

        mapped = MODULE.map_pci_fixups_to_flags(source)

        self.assertEqual(mapped[0][1], [])
        self.assertEqual(MODULE.format_pci_toml(mapped), "")

    def test_handler_name_false_positive_regression_is_ignored(self):
        source = """
static void quirk_pci_dev_flags_no_msi_name_only(struct pci_dev *dev)
{
    pci_info(dev, "name should not drive extraction\n");
}

DECLARE_PCI_FIXUP_FINAL(0x1234, 0x5678, quirk_pci_dev_flags_no_msi_name_only);
"""

        mapped = MODULE.map_pci_fixups_to_flags(source)

        self.assertEqual(mapped[0][1], [])

    def test_class_fixup_carries_class_fields_into_toml_output(self):
        source = """
static void quirk_class_based(struct pci_dev *dev)
{
    dev->dev_flags |= PCI_DEV_FLAGS_ASSIGN_BARS | PCI_DEV_FLAGS_BROKEN_PM;
}

DECLARE_PCI_FIXUP_CLASS_HEADER(PCI_ANY_ID, PCI_ANY_ID, 0x030000, 0xFFFF00, quirk_class_based);
"""

        mapped = MODULE.map_pci_fixups_to_flags(source)
        toml_output = MODULE.format_pci_toml(mapped)

        self.assertEqual(mapped[0][1], ["disable_bar_sizing", "no_pm"])
        self.assertIn("[[pci_quirk]]", toml_output)
        self.assertIn("class = 0x030000", toml_output)
        self.assertIn("class_mask = 0xFFFF00", toml_output)
        self.assertIn('flags = ["disable_bar_sizing", "no_pm"]', toml_output)


if __name__ == "__main__":
    unittest.main()
