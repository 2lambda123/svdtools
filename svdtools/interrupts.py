"""
interrupts.py
Copyright 2018-2020 Adam Greig
Licensed under the MIT and Apache 2.0 licenses. See LICENSE files for details.
"""

import lxml.etree as ET
import lxml.etree


def parse_device(svd_file):
    """Parse device SVD file and extract interrupts.
    Parameters:
        - svd_file (str): Path to SVD file.
    Returns:
        - dname (str): Device name.
        - interrupts (dict): Dictionary of interrupts with their names, descriptions, and parent peripheral names.
    Processing Logic:
        - Parse SVD file using lxml parser.
        - Extract device name from "name" tag.
        - Iterate through "peripheral" tags.
        - Extract peripheral name from "name" tag.
        - Iterate through "interrupt" tags.
        - Extract interrupt name, value, and description.
        - Replace newlines in description with spaces.
        - Store interrupt information in dictionary with value as key.
        - Return device name and interrupts dictionary."""
    
    interrupts = {}
    tree = ET.parse(svd_file, parser=lxml.etree.XMLParser(resolve_entities=False))
    dname = tree.find("name").text
    for ptag in tree.iter("peripheral"):
        pname = ptag.find("name").text
        for itag in ptag.iter("interrupt"):
            name = itag.find("name").text
            value = itag.find("value").text
            maybe_desc = itag.find("description")
            desc = maybe_desc.text.replace("\n", " ") if maybe_desc is not None else ""
            interrupts[int(value)] = {"name": name, "desc": desc, "pname": pname}
    return dname, interrupts


def main(svd_file, gaps=True):
    """Returns a string with interrupt information from the provided svd_file.
    Parameters:
        - svd_file (file): The svd file to be parsed.
        - gaps (bool): Optional parameter to include gaps in the interrupt information. Defaults to True.
    Returns:
        - str: A string with interrupt information.
    Processing Logic:
        - Parses the provided svd_file.
        - Finds any missing interrupts.
        - Returns a string with interrupt information, including gaps if specified.
    Example:
        main("svd_file.xml", gaps=False)
        "1 Timer1: Timer 1 interrupt (in Timer 1)"
        "2 Timer2: Timer 2 interrupt (in Timer 2)"
        "3 ADC: ADC interrupt (in ADC)"
        "4 SPI: SPI interrupt (in SPI)"
        "5 Gaps: 0, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15" """
    
    name, interrupts = parse_device(svd_file)
    missing = set()
    lastint = -1
    results = []
    for val in sorted(interrupts.keys()):
        for v in range(lastint + 1, val):
            missing.add(v)
        lastint = val
        i = interrupts[val]
        results.append(f"{val} {i['name']}: {i['desc']} (in {i['pname']})")
    if gaps:
        results.append("Gaps: " + ", ".join(str(x) for x in sorted(missing)))
    return "\n".join(results)
