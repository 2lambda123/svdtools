use anyhow::{anyhow, Context};
use svd_parser::svd::{Device, Peripheral, PeripheralInfo};
use yaml_rust::yaml::Hash;

use std::{fs::File, io::Read, path::Path};

use super::modify_register_properties;
use super::peripheral::PeripheralExt;
use super::yaml_ext::{AsType, GetVal};
use super::{abspath, matchname, PatchResult, VAL_LVL};
use super::{make_address_block, make_address_blocks, make_cpu, make_interrupt, make_peripheral};

pub struct PerIter<'a, 'b> {
    it: std::slice::IterMut<'a, Peripheral>,
    spec: &'b str,
    check_derived: bool,
}

impl<'a, 'b> Iterator for PerIter<'a, 'b> {
    type Item = &'a mut Peripheral;
    fn next(&mut self) -> Option<Self::Item> {
        for next in self.it.by_ref() {
            if matchname(&next.name, self.spec)
                && !(self.check_derived && next.derived_from.is_some())
            {
                return Some(next);
            }
        }
        None
    }
}

/// Collecting methods for processing device contents
pub trait DeviceExt {
    /// Iterates over all peripherals that match pspec
    fn iter_peripherals<'a, 'b>(
        &'a mut self,
        spec: &'b str,
        check_derived: bool,
    ) -> PerIter<'a, 'b>;

    /// Work through a device, handling all peripherals
    fn process(&mut self, device: &Hash, update_fields: bool) -> PatchResult;

    /// Delete registers matched by rspec inside ptag
    fn delete_peripheral(&mut self, pspec: &str) -> PatchResult;

    /// Create copy of peripheral
    fn copy_peripheral(&mut self, pname: &str, pmod: &Hash, path: &Path) -> PatchResult;

    /// Modify the `cpu` node inside `device` according to `mod`
    fn modify_cpu(&mut self, cmod: &Hash) -> PatchResult;

    /// Modify pspec inside device according to pmod
    fn modify_peripheral(&mut self, pspec: &str, pmod: &Hash) -> PatchResult;

    /// Add pname given by padd to device
    fn add_peripheral(&mut self, pname: &str, padd: &Hash) -> PatchResult;

    /// Remove registers from pname and mark it as derivedFrom pderive.
    /// Update all derivedFrom referencing pname
    fn derive_peripheral(&mut self, pname: &str, pderive: &str) -> PatchResult;

    /// Move registers from pold to pnew.
    /// Update all derivedFrom referencing pold
    fn rebase_peripheral(&mut self, pnew: &str, pold: &str) -> PatchResult;

    /// Work through a peripheral, handling all registers
    fn process_peripheral(
        &mut self,
        pspec: &str,
        peripheral: &Hash,
        update_fields: bool,
    ) -> PatchResult;
}

impl DeviceExt for Device {
    fn iter_peripherals<'a, 'b>(
        &'a mut self,
        spec: &'b str,
        check_derived: bool,
    ) -> PerIter<'a, 'b> {
        // check_derived=True
        PerIter {
            spec,
            check_derived,
            it: self.peripherals.iter_mut(),
        }
    }

    fn process(&mut self, device: &Hash, update_fields: bool) -> PatchResult {
        // Handle any deletions
        for pspec in device.str_vec_iter("_delete") {
            self.delete_peripheral(pspec)
                .with_context(|| format!("Deleting peripheral matched to `{}`", pspec))?;
        }

        // Handle any copied peripherals
        for (pname, val) in device.hash_iter("_copy") {
            let pname = pname.str()?;
            self.copy_peripheral(
                pname,
                val.hash()?,
                Path::new(device.get_str("_path")?.unwrap_or(".")),
            )
            .with_context(|| format!("Copying peripheral `{}`", pname))?;
        }

        // Handle any modifications
        for (key, val) in device.hash_iter("_modify") {
            let key = key.str()?;
            match key {
                "cpu" => self
                    .modify_cpu(val.hash()?)
                    .with_context(|| "Modifying Cpu tag")?,
                "_peripherals" => {
                    for (pspec, pmod) in val.hash()? {
                        let pspec = pspec.str()?;
                        self.modify_peripheral(pspec, pmod.hash()?)
                            .with_context(|| {
                                format!("Modifying peripherals matched to `{}`", pspec)
                            })?;
                    }
                }
                "vendor" => self.vendor = Some(val.str()?.into()),
                "vendorID" => self.vendor_id = Some(val.str()?.into()),
                "name" => self.name = val.str()?.into(),
                "series" => self.series = Some(val.str()?.into()),
                "version" => self.version = val.str()?.into(),
                "description" => self.description = val.str()?.into(),
                "licenseText" => self.license_text = Some(val.str()?.into()),
                "headerSystemFilename" => self.header_system_filename = Some(val.str()?.into()),
                "headerDefinitionsPrefix" => {
                    self.header_definitions_prefix = Some(val.str()?.into())
                }
                "addressUnitBits" => self.address_unit_bits = val.i64()? as u32,
                "width" => self.width = val.i64()? as u32,
                "size" | "access" | "protection" | "resetValue" | "resetMask" => {
                    modify_register_properties(&mut self.default_register_properties, key, val)?;
                }

                _ => self
                    .modify_peripheral(key, val.hash()?)
                    .with_context(|| format!("Modifying peripherals matched to `{}`", key))?,
            }
        }

        // Handle any new peripherals (!)
        for (pname, padd) in device.hash_iter("_add") {
            let pname = pname.str()?;
            self.add_peripheral(pname, padd.hash()?)
                .with_context(|| format!("Adding peripheral `{}`", pname))?;
        }

        // Handle any derived peripherals
        for (pname, pderive) in device.hash_iter("_derive") {
            let pname = pname.str()?;
            let pderive = pderive.str()?;
            self.derive_peripheral(pname, pderive)
                .with_context(|| format!("Deriving peripheral `{}` from `{}`", pname, pderive))?;
        }

        // Handle any rebased peripherals
        for (pname, pold) in device.hash_iter("_rebase") {
            let pname = pname.str()?;
            let pold = pold.str()?;
            self.rebase_peripheral(pname, pold)
                .with_context(|| format!("Rebasing peripheral from `{}` to `{}`", pold, pname))?;
        }

        // Now process all peripherals
        for (periphspec, val) in device {
            let periphspec = periphspec.str()?;
            if !periphspec.starts_with('_') {
                //val["_path"] = device["_path"]; // TODO: check
                self.process_peripheral(periphspec, val.hash()?, update_fields)
                    .with_context(|| format!("According to `{}`", periphspec))?;
            }
        }

        Ok(())
    }

    fn delete_peripheral(&mut self, pspec: &str) -> PatchResult {
        self.peripherals.retain(|p| !(matchname(&p.name, pspec)));
        Ok(())
    }

    fn copy_peripheral(&mut self, pname: &str, pmod: &Hash, path: &Path) -> PatchResult {
        let pcopysrc = pmod
            .get_str("from")?
            .unwrap()
            .split(':')
            .collect::<Vec<_>>();
        let mut new = match pcopysrc.as_slice() {
            [ppath, pcopyname] => {
                let f = File::open(abspath(path, Path::new(ppath))).unwrap();
                let mut contents = String::new();
                (&f).read_to_string(&mut contents).unwrap();
                let filedev = svd_parser::parse(&contents)
                    .with_context(|| format!("Parsing file {}", contents))?;
                filedev
                    .get_peripheral(pcopyname)
                    .ok_or_else(|| anyhow!("peripheral {} not found", pcopyname))?
                    .clone()
            }
            [pcopyname] => {
                let mut new = self
                    .get_peripheral(pcopyname)
                    .ok_or_else(|| anyhow!("peripheral {} not found", pcopyname))?
                    .clone();
                // When copying from a peripheral in the same file, remove any interrupts.
                new.interrupt = Vec::new();
                new
            }
            _ => return Err(anyhow!("Incorrect `from` tag")),
        };
        new.name = pname.into();
        new.derived_from = None;
        if let Some(ptag) = self.get_mut_peripheral(pname) {
            new.base_address = ptag.base_address;
            new.interrupt = std::mem::take(&mut ptag.interrupt);
            *ptag = new;
        } else {
            self.peripherals.push(new)
        }
        Ok(())
    }

    fn modify_cpu(&mut self, cmod: &Hash) -> PatchResult {
        let cpu = make_cpu(cmod)?;
        if let Some(c) = self.cpu.as_mut() {
            c.modify_from(cpu, VAL_LVL)?;
        } else {
            self.cpu = Some(cpu.build(VAL_LVL)?);
        }
        Ok(())
    }

    fn modify_peripheral(&mut self, pspec: &str, pmod: &Hash) -> PatchResult {
        for ptag in self.iter_peripherals(pspec, true) {
            ptag.modify_from(make_peripheral(pmod, true)?, VAL_LVL)?;
            if let Some(ints) = pmod.get_hash("interrupts")? {
                for (iname, val) in ints {
                    let iname = iname.str()?;
                    let int = make_interrupt(val.hash()?)?;
                    for i in &mut ptag.interrupt {
                        if i.name == iname {
                            i.modify_from(int, VAL_LVL)?;
                            break;
                        }
                    }
                }
            }
            if let Some(abmod) = pmod.get_hash("addressBlock").ok().flatten() {
                let v = &mut ptag.address_block;
                let ab = make_address_block(abmod)?;
                match v.as_deref_mut() {
                    Some([adb]) => adb.modify_from(ab, VAL_LVL)?,
                    _ => *v = Some(vec![ab.build(VAL_LVL)?]),
                }
            } else if let Some(abmod) = pmod.get_vec("addressBlocks").ok().flatten() {
                ptag.address_block = Some(make_address_blocks(abmod)?);
            }
        }
        Ok(())
    }

    fn add_peripheral(&mut self, pname: &str, padd: &Hash) -> PatchResult {
        if self.get_peripheral(pname).is_some() {
            return Err(anyhow!("device already has a peripheral {}", pname));
        }

        self.peripherals.push(
            make_peripheral(padd, false)?
                .name(pname.to_string())
                .build(VAL_LVL)?
                .single(),
        );
        Ok(())
    }

    fn derive_peripheral(&mut self, pname: &str, pderive: &str) -> PatchResult {
        self.get_peripheral(pderive)
            .ok_or_else(|| anyhow!("peripheral {} not found", pderive))?;
        self.get_mut_peripheral(pname)
            .ok_or_else(|| anyhow!("peripheral {} not found", pname))?
            .modify_from(
                PeripheralInfo::builder().derived_from(Some(pderive.into())),
                VAL_LVL,
            )?;
        for p in self
            .peripherals
            .iter_mut()
            .filter(|p| p.derived_from.as_deref() == Some(pname))
        {
            p.derived_from = Some(pderive.into());
        }
        Ok(())
    }

    fn rebase_peripheral(&mut self, pnew: &str, pold: &str) -> PatchResult {
        let old = self
            .get_mut_peripheral(pold)
            .ok_or_else(|| anyhow!("peripheral {} not found", pold))?;
        let mut d = std::mem::replace(
            old,
            PeripheralInfo::builder()
                .name(pold.into())
                .base_address(old.base_address)
                .interrupt(if old.interrupt.is_empty() {
                    None
                } else {
                    Some(old.interrupt.clone())
                })
                .derived_from(Some(pnew.into()))
                .build(VAL_LVL)?
                .single(),
        );
        let new = self
            .get_mut_peripheral(pnew)
            .ok_or_else(|| anyhow!("peripheral {} not found", pnew))?;
        d.name = new.name.clone();
        d.base_address = new.base_address;
        d.interrupt = new.interrupt.clone();
        *new = d;
        for p in self
            .peripherals
            .iter_mut()
            .filter(|p| p.derived_from.as_deref() == Some(pold))
        {
            p.derived_from = Some(pnew.into());
        }
        Ok(())
    }

    fn process_peripheral(
        &mut self,
        pspec: &str,
        peripheral: &Hash,
        update_fields: bool,
    ) -> PatchResult {
        // Find all peripherals that match the spec
        let mut pcount = 0;
        for ptag in self.iter_peripherals(pspec, false) {
            pcount += 1;
            ptag.process(peripheral, update_fields)
                .with_context(|| format!("Processing peripheral `{}`", ptag.name))?;
        }
        if pcount == 0 {
            Err(anyhow!("Could not find `{}`", pspec))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils;
    use std::path::Path;

    #[test]
    fn add_peripherals() {
        let (mut device, yaml) = test_utils::get_patcher(Path::new("add")).unwrap();
        assert_eq!(device.peripherals.len(), 1);
        device.process(&yaml, true).unwrap();
        assert_eq!(device.peripherals.len(), 2);
        let periph1 = &device.peripherals[0];
        assert_eq!(periph1.name, "DAC1");
        let periph2 = &device.peripherals[1];
        assert_eq!(periph2.name, "CPUID");
    }

    #[test]
    fn delete_peripherals() {
        let (mut device, yaml) = test_utils::get_patcher(Path::new("delete")).unwrap();
        assert_eq!(device.peripherals.len(), 3);
        device.process(&yaml, true).unwrap();
        assert_eq!(device.peripherals.len(), 1);
        let remaining_periph = &device.peripherals[0];
        assert_eq!(remaining_periph.name, "DAC2");
    }

    #[test]
    fn copy_peripherals() {
        let (mut device, yaml) = test_utils::get_patcher(Path::new("copy")).unwrap();
        assert_eq!(device.peripherals.len(), 3);
        let dac1 = device.get_peripheral("DAC1").unwrap();
        let dac2 = device.get_peripheral("DAC2").unwrap();
        assert_ne!(dac1.registers, dac2.registers);

        device.process(&yaml, true).unwrap();
        assert_eq!(device.peripherals.len(), 3);

        let dac1 = device.get_peripheral("DAC1").unwrap();
        let dac2 = device.get_peripheral("DAC2").unwrap();
        assert_eq!(dac1.registers, dac2.registers);
    }

    #[test]
    fn modify_device() {
        let (mut device, yaml) = test_utils::get_patcher(Path::new("modify")).unwrap();

        // check device initial config
        assert_eq!(&device.version, "1.6");
        assert_eq!(&device.description, "");

        // check cpu initial config
        let cpu = &device.cpu.clone().unwrap();
        assert_eq!(cpu.nvic_priority_bits, 3);

        // check peripheral initial config
        assert_eq!(device.peripherals.len(), 2);
        let dac1 = device.get_peripheral("DAC1").unwrap();
        assert_eq!(dac1.name, "DAC1");
        assert_eq!(dac1.description, None);

        device.process(&yaml, true).unwrap();

        // check device final config
        assert_eq!(&device.version, "1.7");
        assert_eq!(&device.description, "STM32L4x2");

        // check cpu final config
        let cpu = &device.cpu.clone().unwrap();
        assert_eq!(cpu.nvic_priority_bits, 4);

        // check peripheral final config
        let dac1 = device.get_peripheral("DAC11").unwrap();
        assert_eq!(dac1.name, "DAC11");
        assert_eq!(
            dac1.description,
            Some("Digital-to-analog converter".to_string())
        );
    }
}