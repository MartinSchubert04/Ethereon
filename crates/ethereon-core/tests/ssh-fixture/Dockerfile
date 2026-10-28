# Servidor SSH de prueba para tests/acceptance.rs (no es para usar en serio).
FROM alpine:3.20
RUN apk add --no-cache openssh sudo bash procps \
 && adduser -D -s /bin/bash tester && echo 'tester:tester-pass-123' | chpasswd \
 && echo 'tester ALL=(ALL) ALL' > /etc/sudoers.d/tester \
 && ssh-keygen -A \
 && sed -i -E 's/^#?PasswordAuthentication.*/PasswordAuthentication yes/; s/^#?AllowTcpForwarding.*/AllowTcpForwarding yes/' /etc/ssh/sshd_config \
 && echo 'the db secret is Sup3r-S3cret-Value' > /home/tester/notes.txt \
 && mkdir -p /home/tester/.ssh
COPY authorized_keys /home/tester/.ssh/authorized_keys
RUN chown -R tester:tester /home/tester && chmod 700 /home/tester/.ssh && chmod 600 /home/tester/.ssh/authorized_keys
CMD ["/usr/sbin/sshd", "-D", "-e"]
