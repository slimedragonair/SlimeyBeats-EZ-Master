"""Bounded guest work budgets and their public/operator configuration."""
DEFAULT_JOBS_PER_SESSION = 16
DEFAULT_JOBS_PER_IP_HOUR = 32
DEFAULT_JOBS_PER_HOUR = 128
COMMANDS_PER_SESSION = 120


def validate(config):
    for name, maximum in (('guest_jobs_per_session', 256), ('guest_jobs_per_ip_hour', 1024),
                          ('guest_jobs_per_hour', 4096)):
        value = getattr(config, name)
        if type(value) is not int or not 1 <= value <= maximum:
            raise ValueError(name + ' is outside its bounded operator range')


def public_limits(config):
    return {name: getattr(config, name) for name in (
        'guest_jobs_per_session', 'guest_jobs_per_ip_hour', 'guest_jobs_per_hour')} | {
        'guest_admission_commands_per_session': COMMANDS_PER_SESSION}
