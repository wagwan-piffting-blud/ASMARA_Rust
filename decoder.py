import sys
import json
import argparse
from EAS2Text import EAS2Text

def main():
    parser = argparse.ArgumentParser(description="Decode a ZCZC string using EAS2Text and output as JSON.")
    parser.add_argument("--msg", required=True, help="The ZCZC message string to decode.")
    parser.add_argument("--tz", required=False, help="Timezone in TZ database format (e.g., 'America/New_York'). Defaults to 'Etc/UTC'.")
    args = parser.parse_args()

    timezone = args.tz if args.tz else "Etc/UTC"

    try:
        data = EAS2Text(args.msg, timeZoneTZ=timezone)

        output_data = {
            "eas_text": data.EASText,
            "event_text": data.evntText,
            "event_code": data.evnt,
            "fips": data.FIPS,
            "locations": ", ".join(data.FIPSText),
            "originator": data.orgText,
            "timezone": timezone,
        }

        print(json.dumps(output_data))
        sys.exit(0)

    except Exception as e:
        print(json.dumps({"error": str(e)}), file=sys.stderr)
        sys.exit(1)

if __name__ == "__main__":
    main()
