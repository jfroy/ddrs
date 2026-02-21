// SPDX-License-Identifier: GPL-2.0-only
/*
 * amd_smn - Minimal kernel module for AMD SMN (System Management Network)
 *           register reads via PCI config space.
 *
 * SMN registers are accessed indirectly through the AMD root complex:
 *   1. Write the 32-bit SMN address to PCI config offset 0xC4
 *   2. Read the 32-bit value back from PCI config offset 0xC8
 *
 * This module exposes a misc character device /dev/amd_smn with a single
 * ioctl (AMD_SMN_IOC_READ) so unprivileged^W root userspace can read
 * arbitrary SMN addresses.
 */

#include <linux/module.h>
#include <linux/miscdevice.h>
#include <linux/fs.h>
#include <linux/pci.h>
#include <linux/mutex.h>
#include <linux/uaccess.h>

#include "amd_smn.h"

#define SMN_PCI_ADDR_REG 0xC4
#define SMN_PCI_DATA_REG 0xC8

#define AMD_VENDOR_ID 0x1022

static struct pci_dev *root_dev;
static DEFINE_MUTEX(smn_mutex);

static int smn_read(u32 address, u32 *value)
{
	int ret;

	mutex_lock(&smn_mutex);

	ret = pci_write_config_dword(root_dev, SMN_PCI_ADDR_REG, address);
	if (ret) {
		mutex_unlock(&smn_mutex);
		return ret;
	}

	ret = pci_read_config_dword(root_dev, SMN_PCI_DATA_REG, value);

	mutex_unlock(&smn_mutex);
	return ret;
}

static long amd_smn_ioctl(struct file *file, unsigned int cmd,
			   unsigned long arg)
{
	struct amd_smn_req req;
	int ret;

	switch (cmd) {
	case AMD_SMN_IOC_READ:
		if (copy_from_user(&req, (void __user *)arg, sizeof(req)))
			return -EFAULT;

		ret = smn_read(req.address, &req.value);
		if (ret)
			return ret;

		if (copy_to_user((void __user *)arg, &req, sizeof(req)))
			return -EFAULT;

		return 0;

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
	/*
	 * Find the AMD host bridge at bus 0, device 0, function 0.
	 * All Zen-family processors expose SMN through this device.
	 */
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
MODULE_AUTHOR("ddr5timings");
MODULE_DESCRIPTION("AMD SMN register read access via PCI config space");
