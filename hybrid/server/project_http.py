"""Small v2 HTTP router; all authentication/host/origin checks remain shared."""

import re
from urllib.parse import parse_qs

from .client_identity import client_key
from .errors import ApiError
from .project_commands import submit
from .project_limits import CHUNK_BYTES, capabilities
from .project_media import stream_media

PROJECT = re.compile(r"/v2/projects/([a-f0-9]{32})(?:/(.*))?\Z")
UPLOAD = re.compile(r"uploads/([a-f0-9]{32})\Z")
MEDIA = re.compile(r"assets/([a-f0-9]{32})/(audio|preview|waveform)\Z")
ARTIFACT = re.compile(r"artifacts/([a-f0-9]{32})\Z")


def empty_body(handler):
    if handler.body(16384, json_body=True) != {}:
        raise ApiError(400, "invalid_fields")


def dispatch(handler, parsed, principal):
    path, method = parsed.path, handler.command
    store = handler.server.store.projects
    if path == "/v2/capabilities" and not parsed.query and method == "GET":
        handler.json_response(200, capabilities(handler.server.config))
        return
    if path == "/v2/projects" and not parsed.query and method == "POST":
        empty_body(handler)
        handler.json_response(201, store.create({}, principal))
        return
    match = PROJECT.fullmatch(path)
    if not match:
        raise ApiError(404, "endpoint_not_found")
    project_id, action = match.groups()
    # Prove ownership before parsing payloads or disclosing media existence.
    if action is None and method == "DELETE" and not parsed.query:
        handler.json_response(200, store.delete(project_id, principal))
        return
    store.snapshot(project_id, principal)
    upload = UPLOAD.fullmatch(action or "")
    if upload and method == "PUT":
        query = parse_qs(parsed.query, keep_blank_values=True)
        if (set(query) != {"offset"} or len(query["offset"]) != 1
                or not re.fullmatch(r"[0-9]{1,12}", query["offset"][0])):
            raise ApiError(400, "invalid_offset")
        length = handler.body_length(CHUNK_BYTES)
        result = store.upload(project_id, upload[1], int(query["offset"][0]), length, handler.rfile, principal)
        handler.json_response(200, result)
        return
    if parsed.query:
        raise ApiError(400, "query_not_allowed")
    status = 200
    if action is None and method == "GET":
        result = store.snapshot(project_id, principal)
    elif action == "uploads" and method == "POST":
        result = store.create_upload(project_id, handler.body(16384, json_body=True), principal)
        status = 201
    elif action == "commands" and method == "POST":
        result = submit(store, project_id, handler.body(65536, json_body=True), principal,
                        client_key(handler) if not principal.is_owner else None)
        status = 202
    elif action == "cancel" and method == "POST":
        empty_body(handler)
        result = store.cancel(project_id, principal)
    else:
        media, artifact = MEDIA.fullmatch(action or ""), ARTIFACT.fullmatch(action or "")
        if media and method == "GET":
            stream_media(handler, store, project_id, media[1], media[2], principal)
            return
        if artifact and method == "GET":
            stream_media(handler, store, project_id, artifact[1], "artifact", principal)
            return
        raise ApiError(405, "method_not_allowed")
    handler.json_response(status, result)
