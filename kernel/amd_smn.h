#ifndef AMD_SMN_H
#define AMD_SMN_H

#include <linux/ioctl.h>
#include <linux/types.h>

struct amd_smn_req {
	__u32 address;
	__u32 value;
};

#define AMD_SMN_IOC_MAGIC 'S'
#define AMD_SMN_IOC_READ _IOWR(AMD_SMN_IOC_MAGIC, 1, struct amd_smn_req)

#endif /* AMD_SMN_H */
