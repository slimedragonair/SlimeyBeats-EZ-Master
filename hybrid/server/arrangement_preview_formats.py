"""Compressed preview containers share the existing MP3 encoder and frame clock."""
PREVIEW_RATE = 48000
PREVIEW_PRIMING_FRAMES = 1105
PREVIEW_CONTAINERS = ('mp3', 'mp4')
PREVIEW_MIME = {'mp3': 'audio/mpeg', 'mp4': 'audio/mp4; codecs="mp4a.6B"'}
MIN_PREVIEW_SOURCE_NUMERATOR = 3
MIN_PREVIEW_SOURCE_DENOMINATOR = 25


def preview_padding(remaining_frames, sample_rate):
    """Return input-rate zero frames and their ceiling-rounded 48 kHz duration."""
    if type(remaining_frames) is not int or type(sample_rate) is not int or sample_rate <= 0:
        raise ValueError('Invalid preview padding geometry')
    if remaining_frames <= 0:
        return 0, 0
    minimum = (sample_rate * MIN_PREVIEW_SOURCE_NUMERATOR + MIN_PREVIEW_SOURCE_DENOMINATOR - 1) // MIN_PREVIEW_SOURCE_DENOMINATOR
    tail = max(0, minimum - remaining_frames)
    return tail, (tail * PREVIEW_RATE + sample_rate - 1) // sample_rate


def encoder_arguments(config, descriptor):
    # Explicit raw PCM geometry and bounded probing prevent seconds of input
    # analysis before the first live packet. Neither path drops input frames.
    command = [config.ffmpeg, '-nostdin', '-hide_banner', '-loglevel', 'error',
        '-f', 'f32le', '-ar', descriptor['sample_rate'], '-ac', descriptor['channels'],
        '-probesize', '32', '-analyzeduration', '1', '-i', 'pipe:0',
        '-ar', PREVIEW_RATE, '-c:a', 'libmp3lame', '-b:a', f'{config.v2_preview_kbps}k']
    if descriptor.get('preview_container', 'mp3') == 'mp4':
        # Integer packet timestamps avoid raw-MPEG MSE timestamp generation.
        # Keep encoder priming in the stream: no edit-list trim, first DTS=0.
        command += ['-movflags', 'empty_moov+default_base_moof+frag_keyframe',
            '-frag_duration', '100000', '-min_frag_duration', '100000',
            '-avoid_negative_ts', 'make_zero', '-use_editlist', '0',
            '-flush_packets', '1', '-f', 'mp4', 'pipe:1']
    else:
        command += ['-write_xing', '0', '-id3v2_version', '0', '-flush_packets', '1', '-f', 'mp3', 'pipe:1']
    return command
