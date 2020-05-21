#include <netinet/in.h>
#include <sys/socket.h>
#include <netinet/ip.h>
#include <string.h>
#include <unistd.h>

#ifndef IP_RECVORIGDSTADDR
#ifdef  IP_ORIGDSTADDR
#define IP_RECVORIGDSTADDR   IP_ORIGDSTADDR
#else
#define IP_RECVORIGDSTADDR   20
#endif
#endif

#ifndef IPV6_RECVORIGDSTADDR
#ifdef  IPV6_ORIGDSTADDR
#define IPV6_RECVORIGDSTADDR   IPV6_ORIGDSTADDR
#else
#define IPV6_RECVORIGDSTADDR   74
#endif
#endif

int udp_sas_IP_RECVORIGDSTADDR    = IP_RECVORIGDSTADDR;
int udp_sas_IPV6_RECVORIGDSTADDR  = IPV6_RECVORIGDSTADDR;

ssize_t udp_sas_recv(int sock, void* buf, size_t buf_len, int flags,
		struct sockaddr* src, socklen_t src_len,
		struct sockaddr* dst, socklen_t dst_len)
{
	struct iovec iov = {
		.iov_base = buf,
		.iov_len  = buf_len
	};

	char control[64];
	memset(src, 0, src_len);
	memset(dst, 0, dst_len);
	struct msghdr msg = {
		.msg_name	= src,
		.msg_namelen	= src_len,
		.msg_iov	= &iov,
		.msg_iovlen	= 1,
		.msg_control	= control,
		.msg_controllen = sizeof(control),
		.msg_flags	= 0,
	};

	ssize_t nb = recvmsg(sock, &msg, flags);
	if (nb >= 0) {
		// parse the ancillary data
		struct cmsghdr *cmsg;
		for (cmsg = CMSG_FIRSTHDR(&msg); cmsg != 0; cmsg = CMSG_NXTHDR(&msg, cmsg)) {
			// IPv4 destination (IP_PKTINFO)
			// NOTE: may also be present for v4-mapped addresses in IPv6
			if (cmsg->cmsg_level == SOL_IP && cmsg->cmsg_type == IP_RECVORIGDSTADDR
					&& dst_len >= sizeof(struct sockaddr_in)) {
				memcpy(dst, CMSG_DATA(cmsg), sizeof(struct sockaddr_in));
                dst->sa_family = AF_INET;
			}
			// IPv6 destination (IPV6_RECVPKTINFO)
			else if (cmsg->cmsg_level == SOL_IPV6 && cmsg->cmsg_type == IPV6_RECVORIGDSTADDR
					&& dst_len >= sizeof(struct sockaddr_in6)) {
                memcpy(dst, CMSG_DATA(cmsg), sizeof(struct sockaddr_in6));
                dst->sa_family = AF_INET6;
			}
		}

	}
	return nb;
}

int udp_sas_socket(struct sockaddr* bindaddr, socklen_t src_len) {
    int src_fd = socket(bindaddr->sa_family, SOCK_DGRAM, 0);
    if (src_fd < 0) {
        return src_fd;
    }

    int opt  = 1;
    int sol  = bindaddr->sa_family == AF_INET6 ? SOL_IPV6 : SOL_IP;
    int flag = bindaddr->sa_family == AF_INET6 ? IPV6_TRANSPARENT : IP_TRANSPARENT;
    if (setsockopt(src_fd, sol, flag, &opt, sizeof(opt))) {
        close(src_fd);
        return -1;
    }
    if (setsockopt(src_fd, SOL_SOCKET, SO_REUSEADDR, &opt, sizeof(opt))) {
        close(src_fd);
        return -1;
    }

	// bind to any address
    if (bind(src_fd, (struct sockaddr *)bindaddr, src_len) != 0) {
        close(src_fd);
        return -1;
    }

	return src_fd;
}
