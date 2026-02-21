#ifndef AMD_SMN_H
#define AMD_SMN_H

#include <linux/ioctl.h>
#include <linux/types.h>

struct amd_smn_req {
	__u32 address;
	__u32 value;
};

/**
 * struct amd_pm_table_req - Request to read the SMU PM table.
 * @version: (out) PM table version returned by the SMU.
 * @size:    (in) size of the userspace buffer; (out) actual bytes copied.
 * @buffer:  (in) pointer to userspace buffer.
 *
 * The kernel handles the entire SMU mailbox flow internally:
 * get table version, get DRAM base address, transfer table, memremap, copy.
 */
struct amd_pm_table_req {
	__u32 version;
	__u32 size;
	__u64 buffer;
};

#define AMD_SMN_IOC_MAGIC 'S'
#define AMD_SMN_IOC_READ           _IOWR(AMD_SMN_IOC_MAGIC, 1, struct amd_smn_req)
#define AMD_SMN_IOC_WRITE          _IOW(AMD_SMN_IOC_MAGIC, 2, struct amd_smn_req)
#define AMD_SMN_IOC_READ_PM_TABLE  _IOWR(AMD_SMN_IOC_MAGIC, 4, struct amd_pm_table_req)

#endif /* AMD_SMN_H */
