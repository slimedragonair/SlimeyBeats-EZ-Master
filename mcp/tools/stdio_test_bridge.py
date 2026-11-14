"""Generic interactive JSON-RPC bridge for a fresh agent's discovery-only test.

The bridge prints initialize's result and forwards JSON requests from stdin.
It supplies no product workflow. Never use it as the configured stdio MCP server.
"""
import argparse
import json
from protocol_client import Client

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--server", required=True)
parser.add_argument("--input-root", required=True)
parser.add_argument("--output-root", required=True)
args = parser.parse_args()
client = Client([args.server, "--input-root", args.input_root, "--output-root", args.output_root])
try:
    print(json.dumps(client.initialized), flush=True)
    for line in __import__("sys").stdin:
        try:
            value = json.loads(line)
            if value.get("method") == "bridge/close":
                break
            print(json.dumps(client.request(value["method"], value.get("params",{}))), flush=True)
        except Exception as error:
            print(json.dumps({"bridge_error":str(error)}), flush=True)
finally:
    client.close()
