FROM ubuntu:24.04
ENV container=docker
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    systemd systemd-sysv dbus-user-session libpam-systemd openssh-server python3 git ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 1001 --shell /bin/bash fixture \
    && echo 'fixture:fixture-local-only' | chpasswd \
    && mkdir -p /run/sshd /var/lib/systemd/linger \
    && touch /var/lib/systemd/linger/fixture \
    && systemctl enable ssh \
    && rm -f /etc/ssh/ssh_host_*_key /etc/ssh/ssh_host_*_key.pub
RUN printf '%s\n' 'UsePAM yes' 'PasswordAuthentication yes' 'PermitRootLogin no' \
    'AllowUsers fixture' > /etc/ssh/sshd_config.d/fixture.conf
STOPSIGNAL SIGRTMIN+3
COPY start.sh /usr/local/bin/jam-fixture-init
CMD ["/bin/sh", "/usr/local/bin/jam-fixture-init"]
