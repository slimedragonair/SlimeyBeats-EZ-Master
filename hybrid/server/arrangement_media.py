"""Authenticated compressed stream lease; frontend receives no source PCM."""
from .errors import ApiError, ProcessingError


def stream(handler, store, project_id, stream_id, principal):
    with store.lock:
        project = store.get(project_id, principal)
        state = project.arrangement
        if state is None:
            raise ApiError(404, 'arrangement_not_found')
        with state.lock:
            selected = state.stream
            if selected is None or selected.id != stream_id or selected.started:
                raise ApiError(404, 'arrangement_stream_not_found')
            project.downloads += 1
            project.downloads_done.clear()
    try:
        if selected.descriptor['start_frame'] >= selected.descriptor['side_duration_frames']:
            handler.headers_for(200, 0, 'audio/mpeg', extra_headers={'Accept-Ranges': 'none'})
            return
        source = selected.start()
        # Close-delimited HTTP streams need no guessed Content-Length or false
        # byte-range mapping. Seeking prepares a new source-frame stream.
        handler.headers_for(200, None, 'audio/mpeg', extra_headers={'Accept-Ranges': 'none'})
        while not selected.cancel.is_set():
            block = source.read1(8192)
            if not block:
                break
            with store.lock:
                store.get(project_id, principal)
            handler.wfile.write(block)
            handler.wfile.flush()
        if selected.failed:
            handler.close_connection = True
    except (OSError, ValueError, ProcessingError):
        handler.close_connection = True
    finally:
        selected.close()
        with store.lock:
            project.downloads -= 1
            if not project.downloads:
                project.downloads_done.set()
