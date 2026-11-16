"""Resolve client addresses only at the explicitly configured proxy boundary."""

import ipaddress

from .errors import ApiError


def check_host(handler):
    expected = handler.server.config.expected_host
    if expected is not None and handler.headers.get_all("Host", []) != [expected]:
        raise ApiError(400, "Unexpected request host")


def client_key(handler):
    peer = ipaddress.ip_address(handler.client_address[0])
    address = peer
    config = handler.server.config
    header = ("X-SlimeyBeats-Client-IP" if config.trust_relay_client_ip else
              "CF-Connecting-IP" if config.trust_cloudflare_client_ip else None)
    if header is not None:
        # Configuration also checks listener loopback and the fixed public hostname.
        check_host(handler)
        values = handler.headers.get_all(header, [])
        if not peer.is_loopback or len(values) != 1 or "%" in values[0]:
            raise ApiError(400, "One trusted client address is required")
        try:
            address = ipaddress.ip_address(values[0])
        except ValueError as error:
            raise ApiError(400, "One trusted client address is required") from error
    # Raw addresses are never retained as identity metadata or emitted in logs.
    return handler.server.store.sessions.client_key(str(address))
