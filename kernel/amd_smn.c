// SPDX-License-Identifier: GPL-2.0-only
/*
 * amd_smn - Kernel module for AMD SMN (System Management Network) register
 *           access and SMU PM table reading.
 *
 * SMN registers are accessed indirectly through the AMD root complex:
 *   1. Write the 32-bit SMN address to PCI config offset 0xC4
 *   2. Read/write the 32-bit value at PCI config offset 0xC8
 *
 * Ioctls:
 *   AMD_SMN_IOC_READ          - read a 32-bit SMN register
 *   AMD_SMN_IOC_WRITE         - write a 32-bit SMN register
 *   AMD_SMN_IOC_READ_PM_TABLE - read the SMU PM table into a userspace buffer
 */

#include <linux/module.h>
#include <linux/miscdevice.h>
#include <linux/fs.h>
#include <linux/pci.h>
#include <linux/mutex.h>
#include <linux/uaccess.h>
#include <linux/io.h>
#include <linux/slab.h>
#include <linux/delay.h>
#include <linux/capability.h>

#include "amd_smn.h"

#define SMN_PCI_ADDR_REG 0xC4
#define SMN_PCI_DATA_REG 0xC8

#define AMD_VENDOR_ID 0x1022

/* Zen4/Zen5 desktop RSMU mailbox SMN addresses. */
#define RSMU_ADDR_MSG  0x03B10524
#define RSMU_ADDR_RSP  0x03B10570
#define RSMU_ADDR_ARG0 0x03B10A40

#define SMU_CMD_TRANSFER_TABLE    0x3
#define SMU_CMD_GET_DRAM_BASE     0x4
#define SMU_CMD_GET_TABLE_VERSION 0x5

#define SMU_RSP_OK      0x01
#define SMU_TIMEOUT     8192
#define SMU_NUM_ARGS    6

#define PM_TABLE_MAX_SIZE (16 * 1024)

static struct pci_dev *root_dev;
static DEFINE_MUTEX(smn_mutex);

static int smn_read_unlocked(u32 address, u32 *value)
{
	int ret;

	ret = pci_write_config_dword(root_dev, SMN_PCI_ADDR_REG, address);
	if (ret)
		return ret;

	return pci_read_config_dword(root_dev, SMN_PCI_DATA_REG, value);
}

static int smn_write_unlocked(u32 address, u32 value)
{
	int ret;

	ret = pci_write_config_dword(root_dev, SMN_PCI_ADDR_REG, address);
	if (ret)
		return ret;

	return pci_write_config_dword(root_dev, SMN_PCI_DATA_REG, value);
}

static int smn_read(u32 address, u32 *value)
{
	int ret;

	mutex_lock(&smn_mutex);
	ret = smn_read_unlocked(address, value);
	mutex_unlock(&smn_mutex);
	return ret;
}

static int smn_write(u32 address, u32 value)
{
	int ret;

	mutex_lock(&smn_mutex);
	ret = smn_write_unlocked(address, value);
	mutex_unlock(&smn_mutex);
	return ret;
}

/* --- SMU mailbox protocol (caller must hold smn_mutex) --- */

static int smu_wait_done_unlocked(void)
{
	u32 rsp;
	int i, ret;

	for (i = 0; i < SMU_TIMEOUT; i++) {
		ret = smn_read_unlocked(RSMU_ADDR_RSP, &rsp);
		if (ret)
			return ret;
		if (rsp != 0)
			return 0;
		udelay(10);
	}
	return -ETIMEDOUT;
}

/*
 * Send an SMU command and return the response status in *status.
 * args[0..5] are written before the command and read back after.
 */
static int smu_send_command_unlocked(u32 cmd, u32 args[SMU_NUM_ARGS],
				     u32 *status)
{
	int ret, i;

	ret = smu_wait_done_unlocked();
	if (ret)
		return ret;

	ret = smn_write_unlocked(RSMU_ADDR_RSP, 0);
	if (ret)
		return ret;

	for (i = 0; i < SMU_NUM_ARGS; i++) {
		ret = smn_write_unlocked(RSMU_ADDR_ARG0 + i * 4, args[i]);
		if (ret)
			return ret;
	}

	ret = smn_write_unlocked(RSMU_ADDR_MSG, cmd);
	if (ret)
		return ret;

	ret = smu_wait_done_unlocked();
	if (ret)
		return ret;

	ret = smn_read_unlocked(RSMU_ADDR_RSP, status);
	if (ret)
		return ret;

	if (*status == SMU_RSP_OK) {
		for (i = 0; i < SMU_NUM_ARGS; i++) {
			ret = smn_read_unlocked(RSMU_ADDR_ARG0 + i * 4,
						&args[i]);
			if (ret)
				return ret;
		}
	}

	return 0;
}

/* --- ioctl handlers --- */

static long ioctl_smn_read(unsigned long arg)
{
	struct amd_smn_req req;
	int ret;

	if (copy_from_user(&req, (void __user *)arg, sizeof(req)))
		return -EFAULT;

	ret = smn_read(req.address, &req.value);
	if (ret)
		return ret;

	if (copy_to_user((void __user *)arg, &req, sizeof(req)))
		return -EFAULT;

	return 0;
}

static long ioctl_smn_write(unsigned long arg)
{
	struct amd_smn_req req;

	if (copy_from_user(&req, (void __user *)arg, sizeof(req)))
		return -EFAULT;

	return smn_write(req.address, req.value);
}

static long ioctl_read_pm_table(unsigned long arg)
{
	struct amd_pm_table_req req;
	u32 args[SMU_NUM_ARGS] = {};
	u32 status, version;
	u64 dram_base;
	u32 copy_size;
	void *mapped;
	int ret;

	if (copy_from_user(&req, (void __user *)arg, sizeof(req)))
		return -EFAULT;

	if (req.size == 0 || req.size > PM_TABLE_MAX_SIZE)
		return -EINVAL;

	mutex_lock(&smn_mutex);

	/* Get PM table version. */
	memset(args, 0, sizeof(args));
	ret = smu_send_command_unlocked(SMU_CMD_GET_TABLE_VERSION, args,
					&status);
	if (ret)
		goto unlock;
	if (status != SMU_RSP_OK) {
		ret = -EIO;
		goto unlock;
	}
	version = args[0];

	/* Get DRAM base address for the PM table. */
	memset(args, 0, sizeof(args));
	ret = smu_send_command_unlocked(SMU_CMD_GET_DRAM_BASE, args, &status);
	if (ret)
		goto unlock;
	if (status != SMU_RSP_OK || args[0] == 0) {
		ret = -EIO;
		goto unlock;
	}
	dram_base = args[0];

	/* Transfer the PM table to DRAM. */
	memset(args, 0, sizeof(args));
	ret = smu_send_command_unlocked(SMU_CMD_TRANSFER_TABLE, args, &status);
	if (ret)
		goto unlock;
	if (status != SMU_RSP_OK) {
		ret = -EIO;
		goto unlock;
	}

	mutex_unlock(&smn_mutex);

	/* Map physical memory and copy to userspace. */
	mapped = memremap(dram_base, req.size, MEMREMAP_WB);
	if (!mapped)
		return -ENOMEM;

	copy_size = req.size;
	if (copy_to_user((void __user *)req.buffer, mapped, copy_size)) {
		memunmap(mapped);
		return -EFAULT;
	}
	memunmap(mapped);

	/* Write version and actual size back to userspace. */
	req.version = version;
	req.size = copy_size;
	if (copy_to_user((void __user *)arg, &req, sizeof(req)))
		return -EFAULT;

	return 0;

unlock:
	mutex_unlock(&smn_mutex);
	return ret;
}

static long amd_smn_ioctl(struct file *file, unsigned int cmd,
			   unsigned long arg)
{
	if (!capable(CAP_SYS_ADMIN))
		return -EPERM;

	switch (cmd) {
	case AMD_SMN_IOC_READ:
		return ioctl_smn_read(arg);
	case AMD_SMN_IOC_WRITE:
		return ioctl_smn_write(arg);
	case AMD_SMN_IOC_READ_PM_TABLE:
		return ioctl_read_pm_table(arg);
	default:
		return -ENOTTY;
	}
}

static const struct file_operations amd_smn_fops = {
	.owner = THIS_MODULE,
	.unlocked_ioctl = amd_smn_ioctl,
	.compat_ioctl = compat_ptr_ioctl,
};

static struct miscdevice amd_smn_misc = {
	.minor = MISC_DYNAMIC_MINOR,
	.name = "amd_smn",
	.fops = &amd_smn_fops,
	.mode = 0600,
};

static int __init amd_smn_init(void)
{
	root_dev = pci_get_domain_bus_and_slot(0, 0, PCI_DEVFN(0, 0));
	if (!root_dev) {
		pr_err("amd_smn: AMD host bridge not found\n");
		return -ENODEV;
	}

	if (root_dev->vendor != AMD_VENDOR_ID) {
		pr_err("amd_smn: device 0000:00:00.0 is not AMD (vendor 0x%04x)\n",
		       root_dev->vendor);
		pci_dev_put(root_dev);
		return -ENODEV;
	}

	pr_info("amd_smn: using %s [%04x:%04x]\n",
		pci_name(root_dev), root_dev->vendor, root_dev->device);

	return misc_register(&amd_smn_misc);
}

static void __exit amd_smn_exit(void)
{
	misc_deregister(&amd_smn_misc);
	if (root_dev)
		pci_dev_put(root_dev);
}

module_init(amd_smn_init);
module_exit(amd_smn_exit);

MODULE_LICENSE("GPL");
MODULE_AUTHOR("ddrs");
MODULE_DESCRIPTION("AMD SMN register access and SMU PM table reading");
