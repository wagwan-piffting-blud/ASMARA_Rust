import os
import sys
import json
import argparse
from EAS2Text import EAS2Text

def main():
    parser = argparse.ArgumentParser(description="Decode a ZCZC string using EAS2Text and output as JSON.")
    parser.add_argument("--msg", required=True, help="The ZCZC message string to decode.")
    args = parser.parse_args()

    try:
        data = EAS2Text(args.msg, timeZoneTZ=os.getenv("TZ", "Etc/UTC"))

        output_data = {
            "eas_text": data.EASText,
            "event_text": data.evntText,
            "event_code": data.evnt,
            "fips": data.FIPS,
            "locations": ", ".join(data.FIPSText),
            "originator": data.orgText
        }

        print(json.dumps(output_data))
        sys.exit(0)

    except Exception as e:
        print(json.dumps({"error": str(e)}), file=sys.stderr)
        sys.exit(1)

if __name__ == "__main__":
    main()
