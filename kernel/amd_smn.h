#ifndef AMD_SMN_H
#define AMD_SMN_H

#include <linux/ioctl.h>
#include <linux/types.h>

struct amd_smn_req {
	__u32 address;
	__u32 value;
};

struct amd_phys_req {
	__u64 address;
	__u32 size;
	__u32 _pad;
	__u64 buffer;
};

#define AMD_SMN_IOC_MAGIC 'S'
#define AMD_SMN_IOC_READ       _IOWR(AMD_SMN_IOC_MAGIC, 1, struct amd_smn_req)
#define AMD_SMN_IOC_WRITE      _IOW(AMD_SMN_IOC_MAGIC, 2, struct amd_smn_req)
#define AMD_SMN_IOC_READ_PHYS  _IOW(AMD_SMN_IOC_MAGIC, 3, struct amd_phys_req)

#endif /* AMD_SMN_H */
