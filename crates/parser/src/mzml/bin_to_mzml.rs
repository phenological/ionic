use std::fmt::{Display, Formatter};

use base64::{Engine, engine::general_purpose::STANDARD};
use miniz_oxide::deflate::compress_to_vec_zlib;
use quick_xml::{
    Writer,
    events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event},
};
use sha1::{Digest, Sha1};

use crate::{
    accessions::{
        ACC_ANALYZER_QUAD, ACC_ANALYZER_TOF, ACC_COMPRESSION_NONE, ACC_COMPRESSION_ZLIB,
        ACC_DETECTOR_EM, ACC_DETECTOR_PHOTOMULT, ACC_FLOAT_16BIT_STR, ACC_FLOAT_32BIT_STR,
        ACC_FLOAT_64BIT_STR, ACC_INT_16BIT_STR, ACC_INT_32BIT_STR, ACC_INT_64BIT_STR,
        ACC_SOURCE_EI, ACC_SOURCE_ESI,
    },
    mzml::structs::*,
};

#[derive(Debug)]
pub enum BinToMzmlError {
    Io(std::io::Error),
    Xml(quick_xml::Error),
    MissingElement(&'static str),
    InvalidData(&'static str),
}

impl Display for BinToMzmlError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {e}"),
            Self::Xml(e) => write!(f, "XML write error: {e}"),
            Self::MissingElement(s) => write!(f, "missing element: {s}"),
            Self::InvalidData(s) => write!(f, "invalid data: {s}"),
        }
    }
}

impl std::error::Error for BinToMzmlError {}

impl From<std::io::Error> for BinToMzmlError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<quick_xml::Error> for BinToMzmlError {
    fn from(e: quick_xml::Error) -> Self {
        Self::Xml(e)
    }
}

#[inline]
fn write_list<T, F>(
    writer: &mut Writer<Vec<u8>>,
    tag_name: &str,
    count: usize,
    items: &[T],
    mut write_item: F,
) -> Result<(), BinToMzmlError>
where
    F: FnMut(&mut Writer<Vec<u8>>, &T) -> Result<(), BinToMzmlError>,
{
    let mut tag = BytesStart::new(tag_name);
    let mut buf = itoa::Buffer::new();
    tag.push_attribute(("count", buf.format(count)));
    writer.write_event(Event::Start(tag))?;
    for item in items {
        write_item(writer, item)?;
    }
    writer.write_event(Event::End(BytesEnd::new(tag_name)))?;
    Ok(())
}

#[derive(Default)]
struct IndexAcc {
    spectrum: Vec<IndexOffsetAcc>,
    chromatogram: Vec<IndexOffsetAcc>,
}

struct IndexOffsetAcc {
    id_ref: String,
    offset: u64,
}

#[inline]
fn nonempty(s: Option<&str>) -> Option<&str> {
    match s {
        Some(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

#[inline]
fn write_start_capture_offset(
    writer: &mut Writer<Vec<u8>>,
    tag: BytesStart<'_>,
) -> Result<u64, BinToMzmlError> {
    let before = writer.get_ref().len();
    writer.write_event(Event::Start(tag))?;
    let after = writer.get_ref().len();

    let buf = writer.get_ref();
    let rel = buf[before..after].iter().position(|&b| b == b'<').ok_or(
        BinToMzmlError::MissingElement("could not find '<' for start tag"),
    )?;

    Ok((before + rel) as u64)
}

pub fn bin_to_mzml(mzml: &MzML) -> Result<Vec<u8>, BinToMzmlError> {
    let mut writer = Writer::new_with_indent(Vec::new(), b' ', 2);

    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("utf-8"), None)))?;

    let mut idx_tag = BytesStart::new("indexedmzML");
    idx_tag.push_attribute(("xmlns", "http://psi.hupo.org/ms/mzml"));
    idx_tag.push_attribute(("xmlns:xsi", "http://www.w3.org/2001/XMLSchema-instance"));
    idx_tag.push_attribute((
        "xsi:schemaLocation",
        "http://psi.hupo.org/ms/mzml http://psidev.info/files/ms/mzML/xsd/mzML1.1.2_idx.xsd",
    ));
    writer.write_event(Event::Start(idx_tag))?;

    let mut mzml_tag = BytesStart::new("mzML");
    mzml_tag.push_attribute(("xmlns", "http://psi.hupo.org/ms/mzml"));
    mzml_tag.push_attribute(("xmlns:xsi", "http://www.w3.org/2001/XMLSchema-instance"));
    mzml_tag.push_attribute((
        "xsi:schemaLocation",
        "http://psi.hupo.org/ms/mzml http://psidev.info/files/ms/mzML/xsd/mzML1.1.0.xsd",
    ));
    mzml_tag.push_attribute(("id", mzml.run.id.as_str()));
    mzml_tag.push_attribute(("version", "1.1.0"));

    writer.write_event(Event::Start(mzml_tag))?;

    let mut fallback_cvl: Option<CvList> = None;

    let cvl: &CvList = mzml
        .cv_list
        .as_ref()
        .unwrap_or_else(|| fallback_cvl.get_or_insert_with(default_cv_list));

    write_cv_list(&mut writer, cvl)?;

    write_file_description(
        &mut writer,
        mzml.file_description
            .as_ref()
            .ok_or(BinToMzmlError::MissingElement(
                "mzML is missing required <fileDescription> element",
            ))?,
    )?;

    if let Some(rpgl) = &mzml.referenceable_param_group_list {
        write_referenceable_param_group_list(&mut writer, rpgl)?;
    }
    if let Some(sl) = &mzml.sample_list {
        write_sample_list(&mut writer, sl)?;
    }
    if let Some(sw) = &mzml.software_list {
        write_software_list(&mut writer, sw)?;
    }
    if let Some(ssl) = &mzml.scan_settings_list {
        write_scan_settings_list(&mut writer, ssl)?;
    }
    if let Some(il) = &mzml.instrument_list {
        write_instrument_list(&mut writer, il)?;
    }
    if let Some(dpl) = &mzml.data_processing_list {
        write_data_processing_list(&mut writer, dpl)?;
    }

    let fallback_default_dp = mzml
        .data_processing_list
        .as_ref()
        .and_then(|dpl| dpl.data_processing.first())
        .map(|dp| dp.id.as_str());

    let mut idx = IndexAcc::default();
    write_run(&mut writer, &mzml.run, fallback_default_dp, &mut idx)?;

    writer.write_event(Event::End(BytesEnd::new("mzML")))?;

    let index_list_offset = write_index_list_with_offset(&mut writer, &idx)?;
    write_index_list_offset(&mut writer, index_list_offset)?;
    write_file_checksum(&mut writer)?;

    writer.write_event(Event::End(BytesEnd::new("indexedmzML")))?;

    let output = writer.into_inner();
    if first_illegal_xml_byte(&output).is_some() {
        return Err(BinToMzmlError::InvalidData(
            "output contains an XML-1.0-illegal control character",
        ));
    }
    Ok(output)
}

fn first_illegal_xml_byte(bytes: &[u8]) -> Option<(usize, u8)> {
    bytes
        .iter()
        .enumerate()
        .find(|&(_, &b)| matches!(b, 0x00..=0x08 | 0x0B | 0x0C | 0x0E..=0x1F))
        .map(|(i, &b)| (i, b))
}

fn default_cv_list() -> CvList {
    CvList {
        count: Some(2),
        cv: vec![
            CvEntry {
                id: "MS".to_string(),
                full_name: Some(
                    "Proteomics Standards Initiative Mass Spectrometry Ontology".to_string(),
                ),
                version: Some("4.1.182".to_string()),
                uri: Some(
                    "https://raw.githubusercontent.com/HUPO-PSI/psi-ms-CV/master/psi-ms.obo"
                        .to_string(),
                ),
            },
            CvEntry {
                id: "UO".to_string(),
                full_name: Some("Unit Ontology".to_string()),
                version: Some("09:04:2014".to_string()),
                uri: Some(
                    "https://raw.githubusercontent.com/bio-ontology-research-group/unit-ontology/master/unit.obo"
                        .to_string(),
                ),
            },
        ],
    }
}

pub fn write_cv_list(writer: &mut Writer<Vec<u8>>, cvl: &CvList) -> Result<(), BinToMzmlError> {
    let count = cvl.count.unwrap_or(cvl.cv.len());
    let mut tag = BytesStart::new("cvList");
    let count_s = count.to_string();
    tag.push_attribute(("count", count_s.as_str()));

    writer.write_event(Event::Start(tag))?;

    for cv in &cvl.cv {
        let mut cv_tag = BytesStart::new("cv");
        cv_tag.push_attribute(("id", cv.id.as_str()));
        if let Some(v) = &cv.full_name {
            cv_tag.push_attribute(("fullName", v.as_str()));
        }
        if let Some(v) = &cv.version {
            cv_tag.push_attribute(("version", v.as_str()));
        }
        if let Some(v) = &cv.uri {
            cv_tag.push_attribute(("URI", v.as_str()));
        }

        writer.write_event(Event::Empty(cv_tag))?;
    }

    writer.write_event(Event::End(BytesEnd::new("cvList")))?;
    Ok(())
}

fn write_file_description(
    writer: &mut Writer<Vec<u8>>,
    fd: &FileDescription,
) -> Result<(), BinToMzmlError> {
    writer.write_event(Event::Start(BytesStart::new("fileDescription")))?;

    writer.write_event(Event::Start(BytesStart::new("fileContent")))?;

    write_referenceable_param_group_refs(writer, &fd.file_content.referenceable_param_group_refs)?;
    write_cv_params(writer, &fd.file_content.cv_params)?;
    write_user_params(writer, &fd.file_content.user_params)?;

    writer.write_event(Event::End(BytesEnd::new("fileContent")))?;

    if !fd.source_file_list.source_file.is_empty() {
        write_source_file_list(writer, &fd.source_file_list)?;
    }

    for c in &fd.contacts {
        writer.write_event(Event::Start(BytesStart::new("contact")))?;
        write_referenceable_param_group_refs(writer, &c.referenceable_param_group_refs)?;
        write_cv_params(writer, &c.cv_params)?;
        write_user_params(writer, &c.user_params)?;
        writer.write_event(Event::End(BytesEnd::new("contact")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("fileDescription")))?;
    Ok(())
}

fn write_source_file_list(
    writer: &mut Writer<Vec<u8>>,
    sfl: &SourceFileList,
) -> Result<(), BinToMzmlError> {
    write_list(
        writer,
        "sourceFileList",
        sfl.count.unwrap_or(sfl.source_file.len()),
        &sfl.source_file,
        |writer, sf| {
            let mut sf_tag = BytesStart::new("sourceFile");
            sf_tag.push_attribute(("id", sf.id.as_str()));
            if !sf.name.is_empty() {
                sf_tag.push_attribute(("name", sf.name.as_str()));
            }
            if !sf.location.is_empty() {
                sf_tag.push_attribute(("location", sf.location.as_str()));
            }
            writer.write_event(Event::Start(sf_tag))?;
            write_referenceable_param_group_refs(writer, &sf.referenceable_param_group_ref)?;
            write_cv_params(writer, &sf.cv_param)?;
            write_user_params(writer, &sf.user_param)?;
            writer.write_event(Event::End(BytesEnd::new("sourceFile")))?;
            Ok(())
        },
    )
}

fn write_referenceable_param_group_list(
    writer: &mut Writer<Vec<u8>>,
    list: &ReferenceableParamGroupList,
) -> Result<(), BinToMzmlError> {
    write_list(
        writer,
        "referenceableParamGroupList",
        list.count.unwrap_or(list.referenceable_param_groups.len()),
        &list.referenceable_param_groups,
        |writer, g| {
            let mut g_tag = BytesStart::new("referenceableParamGroup");
            g_tag.push_attribute(("id", g.id.as_str()));
            writer.write_event(Event::Start(g_tag))?;
            write_cv_params(writer, &g.cv_params)?;
            write_user_params(writer, &g.user_params)?;
            writer.write_event(Event::End(BytesEnd::new("referenceableParamGroup")))?;
            Ok(())
        },
    )
}

fn write_sample_list(
    writer: &mut Writer<Vec<u8>>,
    list: &SampleList,
) -> Result<(), BinToMzmlError> {
    let count = list.count.unwrap_or(list.samples.len() as u32) as usize;
    let mut tag = BytesStart::new("sampleList");
    let count_s = count.to_string();
    tag.push_attribute(("count", count_s.as_str()));

    writer.write_event(Event::Start(tag))?;

    write_cv_params(writer, &list.cv_params)?;
    write_user_params(writer, &list.user_params)?;

    for s in &list.samples {
        let mut s_tag = BytesStart::new("sample");
        s_tag.push_attribute(("id", s.id.as_str()));
        if !s.name.is_empty() {
            s_tag.push_attribute(("name", s.name.as_str()));
        }
        writer.write_event(Event::Start(s_tag))?;

        for r in &s.referenceable_param_group_refs {
            write_referenceable_param_group_ref(writer, r)?;
        }

        write_cv_params(writer, &s.cv_params)?;
        write_user_params(writer, &s.user_params)?;

        writer.write_event(Event::End(BytesEnd::new("sample")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("sampleList")))?;
    Ok(())
}

fn write_instrument_list(
    writer: &mut Writer<Vec<u8>>,
    list: &InstrumentList,
) -> Result<(), BinToMzmlError> {
    let count = list.count.unwrap_or(list.instrument.len());
    let mut tag = BytesStart::new("instrumentConfigurationList");
    let mut buf = itoa::Buffer::new();
    tag.push_attribute(("count", buf.format(count)));
    writer.write_event(Event::Start(tag))?;

    for ic in &list.instrument {
        let mut ic_tag = BytesStart::new("instrumentConfiguration");
        ic_tag.push_attribute(("id", ic.id.as_str()));
        if let Some(ssr) = &ic.scan_settings_ref
            && let Some(v) = nonempty(Some(ssr.r#ref.as_str()))
        {
            ic_tag.push_attribute(("scanSettingsRef", v));
        }
        writer.write_event(Event::Start(ic_tag))?;
        write_referenceable_param_group_refs(writer, &ic.referenceable_param_group_ref)?;

        let has_fallback_components = ic.cv_param.iter().any(|p| {
            matches!(
                p.accession.as_deref().unwrap_or(""),
                ACC_SOURCE_ESI
                    | ACC_SOURCE_EI
                    | ACC_ANALYZER_QUAD
                    | ACC_ANALYZER_TOF
                    | ACC_DETECTOR_EM
                    | ACC_DETECTOR_PHOTOMULT
            )
        });

        for p in &ic.cv_param {
            if has_fallback_components
                && matches!(
                    p.accession.as_deref().unwrap_or(""),
                    ACC_SOURCE_ESI
                        | ACC_SOURCE_EI
                        | ACC_ANALYZER_QUAD
                        | ACC_ANALYZER_TOF
                        | ACC_DETECTOR_EM
                        | ACC_DETECTOR_PHOTOMULT
                )
            {
                continue;
            }
            write_cv_param(writer, p)?;
        }
        write_user_params(writer, &ic.user_param)?;

        if let Some(cl) = &ic.component_list {
            write_component_list(writer, cl)?;
        } else if has_fallback_components {
            write_component_list_fallback_from_instrument_cv(writer, &ic.cv_param)?;
        }

        if let Some(sw) = &ic.software_ref {
            let mut sw_tag = BytesStart::new("softwareRef");
            sw_tag.push_attribute(("ref", sw.r#ref.as_str()));
            writer.write_event(Event::Empty(sw_tag))?;
        }

        writer.write_event(Event::End(BytesEnd::new("instrumentConfiguration")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("instrumentConfigurationList")))?;
    Ok(())
}

fn write_component_list(
    writer: &mut Writer<Vec<u8>>,
    cl: &ComponentList,
) -> Result<(), BinToMzmlError> {
    let count = cl
        .count
        .unwrap_or(cl.source.len() + cl.analyzer.len() + cl.detector.len());
    let mut tag = BytesStart::new("componentList");
    let count_s = count.to_string();
    tag.push_attribute(("count", count_s.as_str()));

    writer.write_event(Event::Start(tag))?;

    for s in &cl.source {
        write_component(
            writer,
            "source",
            s.order,
            &s.referenceable_param_group_ref,
            &s.cv_param,
            &s.user_param,
        )?;
    }
    for a in &cl.analyzer {
        write_component(
            writer,
            "analyzer",
            a.order,
            &a.referenceable_param_group_ref,
            &a.cv_param,
            &a.user_param,
        )?;
    }
    for d in &cl.detector {
        write_component(
            writer,
            "detector",
            d.order,
            &d.referenceable_param_group_ref,
            &d.cv_param,
            &d.user_param,
        )?;
    }

    writer.write_event(Event::End(BytesEnd::new("componentList")))?;
    Ok(())
}

fn write_component_list_fallback_from_instrument_cv(
    writer: &mut Writer<Vec<u8>>,
    params: &[CvParam],
) -> Result<(), BinToMzmlError> {
    let mut tag = BytesStart::new("componentList");
    tag.push_attribute(("count", "3"));
    writer.write_event(Event::Start(tag))?;

    let source_cvs: Vec<CvParam> = params
        .iter()
        .filter(|p| {
            matches!(
                p.accession.as_deref().unwrap_or(""),
                ACC_SOURCE_ESI | ACC_SOURCE_EI
            )
        })
        .cloned()
        .collect();
    write_component(writer, "source", Some(1), &[], &source_cvs, &[])?;

    let analyzer_cvs: Vec<CvParam> = params
        .iter()
        .filter(|p| {
            matches!(
                p.accession.as_deref().unwrap_or(""),
                ACC_ANALYZER_QUAD | ACC_ANALYZER_TOF
            )
        })
        .cloned()
        .collect();
    write_component(writer, "analyzer", Some(2), &[], &analyzer_cvs, &[])?;

    let detector_cvs: Vec<CvParam> = params
        .iter()
        .filter(|p| {
            matches!(
                p.accession.as_deref().unwrap_or(""),
                ACC_DETECTOR_EM | ACC_DETECTOR_PHOTOMULT
            )
        })
        .cloned()
        .collect();
    write_component(writer, "detector", Some(3), &[], &detector_cvs, &[])?;

    writer.write_event(Event::End(BytesEnd::new("componentList")))?;
    Ok(())
}

fn write_component(
    writer: &mut Writer<Vec<u8>>,
    name: &str,
    order: Option<u32>,
    refs: &[ReferenceableParamGroupRef],
    cvs: &[CvParam],
    ups: &[UserParam],
) -> Result<(), BinToMzmlError> {
    let mut tag = BytesStart::new(name);
    if let Some(o) = order {
        let o_s = o.to_string();
        tag.push_attribute(("order", o_s.as_str()));
    }

    writer.write_event(Event::Start(tag))?;

    write_referenceable_param_group_refs(writer, refs)?;
    write_cv_params(writer, cvs)?;
    write_user_params(writer, ups)?;

    writer.write_event(Event::End(BytesEnd::new(name)))?;
    Ok(())
}

fn write_software_list(
    writer: &mut Writer<Vec<u8>>,
    list: &SoftwareList,
) -> Result<(), BinToMzmlError> {
    let count = list.count.unwrap_or(list.software.len());
    let mut tag = BytesStart::new("softwareList");
    let mut buf = itoa::Buffer::new();
    tag.push_attribute(("count", buf.format(count)));
    writer.write_event(Event::Start(tag))?;

    for sw in &list.software {
        let mut sw_tag = BytesStart::new("software");
        sw_tag.push_attribute(("id", sw.id.as_str()));
        if let Some(v) = &sw.version {
            sw_tag.push_attribute(("version", v.as_str()));
        }
        writer.write_event(Event::Start(sw_tag))?;

        write_referenceable_param_group_refs(writer, &sw.referenceable_param_group_refs)?;

        for sp in &sw.software_param {
            let mut sp_tag = BytesStart::new("softwareParam");
            if let Some(v) = &sp.cv_ref {
                sp_tag.push_attribute(("cvRef", v.as_str()));
            }
            sp_tag.push_attribute(("accession", sp.accession.as_str()));
            sp_tag.push_attribute(("name", sp.name.as_str()));
            if let Some(v) = &sp.version {
                sp_tag.push_attribute(("version", v.as_str()));
            }
            writer.write_event(Event::Empty(sp_tag))?;
        }

        write_cv_params(writer, &sw.cv_param)?;
        write_user_params(writer, &sw.user_params)?;

        writer.write_event(Event::End(BytesEnd::new("software")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("softwareList")))?;
    Ok(())
}

fn write_data_processing_list(
    writer: &mut Writer<Vec<u8>>,
    list: &DataProcessingList,
) -> Result<(), BinToMzmlError> {
    write_list(
        writer,
        "dataProcessingList",
        list.count.unwrap_or(list.data_processing.len()),
        &list.data_processing,
        |writer, dp| {
            let mut dp_tag = BytesStart::new("dataProcessing");
            dp_tag.push_attribute(("id", dp.id.as_str()));
            if let Some(sw) = nonempty(dp.software_ref.as_deref()) {
                dp_tag.push_attribute(("softwareRef", sw));
            }
            writer.write_event(Event::Start(dp_tag))?;
            for m in &dp.processing_method {
                let mut pm = BytesStart::new("processingMethod");
                if let Some(order) = m.order {
                    let mut buf = itoa::Buffer::new();
                    pm.push_attribute(("order", buf.format(order)));
                }
                if let Some(sw) = nonempty(m.software_ref.as_deref()) {
                    pm.push_attribute(("softwareRef", sw));
                }
                writer.write_event(Event::Start(pm))?;
                write_referenceable_param_group_refs(writer, &m.referenceable_param_group_ref)?;
                write_cv_params(writer, &m.cv_param)?;
                write_user_params(writer, &m.user_param)?;
                writer.write_event(Event::End(BytesEnd::new("processingMethod")))?;
            }
            writer.write_event(Event::End(BytesEnd::new("dataProcessing")))?;
            Ok(())
        },
    )
}

fn write_scan_settings_list(
    writer: &mut Writer<Vec<u8>>,
    list: &ScanSettingsList,
) -> Result<(), BinToMzmlError> {
    write_list(
        writer,
        "scanSettingsList",
        list.count.unwrap_or(list.scan_settings.len()),
        &list.scan_settings,
        |writer, ss| {
            let mut ss_tag = BytesStart::new("scanSettings");
            if let Some(id) = ss.id.as_deref()
                && !id.is_empty()
            {
                ss_tag.push_attribute(("id", id));
            }
            if let Some(r) = ss.instrument_configuration_ref.as_deref()
                && !r.is_empty()
            {
                ss_tag.push_attribute(("instrumentConfigurationRef", r));
            }
            writer.write_event(Event::Start(ss_tag))?;
            write_referenceable_param_group_refs(writer, &ss.referenceable_param_group_refs)?;
            write_cv_params(writer, &ss.cv_params)?;
            write_user_params(writer, &ss.user_params)?;
            if let Some(sfrl) = &ss.source_file_ref_list {
                write_source_file_ref_list(writer, sfrl)?;
            }
            if let Some(tl) = &ss.target_list {
                write_target_list(writer, tl)?;
            }
            writer.write_event(Event::End(BytesEnd::new("scanSettings")))?;
            Ok(())
        },
    )
}

fn write_run(
    writer: &mut Writer<Vec<u8>>,
    run: &Run,
    fallback_default_dp: Option<&str>,
    idx: &mut IndexAcc,
) -> Result<(), BinToMzmlError> {
    let mut run_tag = BytesStart::new("run");
    run_tag.push_attribute(("id", run.id.as_str()));
    if let Some(ts) = nonempty(run.start_time_stamp.as_deref()) {
        run_tag.push_attribute(("startTimeStamp", ts));
    }
    if let Some(ic) = nonempty(run.default_instrument_configuration_ref.as_deref()) {
        run_tag.push_attribute(("defaultInstrumentConfigurationRef", ic));
    }
    if let Some(sf) = nonempty(run.default_source_file_ref.as_deref()) {
        run_tag.push_attribute(("defaultSourceFileRef", sf));
    }
    if let Some(samp) = nonempty(run.sample_ref.as_deref()) {
        run_tag.push_attribute(("sampleRef", samp));
    }

    writer.write_event(Event::Start(run_tag))?;

    write_referenceable_param_group_refs(writer, &run.referenceable_param_group_refs)?;
    write_cv_params(writer, &run.cv_params)?;
    write_user_params(writer, &run.user_params)?;

    if let Some(sfrl) = &run.source_file_ref_list {
        write_source_file_ref_list(writer, sfrl)?;
    }
    if let Some(sl) = &run.spectrum_list {
        write_spectrum_list(writer, sl, fallback_default_dp, idx)?;
    }
    if let Some(cl) = &run.chromatogram_list {
        write_chromatogram_list(writer, cl, fallback_default_dp, idx)?;
    }

    writer.write_event(Event::End(BytesEnd::new("run")))?;
    Ok(())
}

fn write_spectrum_list(
    writer: &mut Writer<Vec<u8>>,
    list: &SpectrumList,
    fallback_default_dp: Option<&str>,
    idx: &mut IndexAcc,
) -> Result<(), BinToMzmlError> {
    let count = list.count.unwrap_or(list.spectra.len());
    let mut tag = BytesStart::new("spectrumList");
    let count_s = count.to_string();
    tag.push_attribute(("count", count_s.as_str()));

    if let Some(dp) = nonempty(list.default_data_processing_ref.as_deref()) {
        tag.push_attribute(("defaultDataProcessingRef", dp));
    }

    writer.write_event(Event::Start(tag))?;

    for s in &list.spectra {
        write_spectrum(writer, s, fallback_default_dp, idx)?;
    }

    writer.write_event(Event::End(BytesEnd::new("spectrumList")))?;
    Ok(())
}

fn write_spectrum(
    writer: &mut Writer<Vec<u8>>,
    s: &Spectrum,
    fallback_default_dp: Option<&str>,
    idx: &mut IndexAcc,
) -> Result<(), BinToMzmlError> {
    let mut tag = BytesStart::new("spectrum");

    if let Some(idx0) = s.index {
        let idx_s = idx0.to_string();
        tag.push_attribute(("index", idx_s.as_str()));
    }

    let id_to_write = nonempty(Some(s.id.as_str()))
        .or_else(|| nonempty(s.native_id.as_deref()))
        .unwrap_or(s.id.as_str());
    tag.push_attribute(("id", id_to_write));

    if let Some(default_len) = s.default_array_length {
        let len_s = default_len.to_string();
        tag.push_attribute(("defaultArrayLength", len_s.as_str()));
    }

    if let Some(sn) = s.scan_number {
        let sn_s = sn.to_string();
        tag.push_attribute(("scanNumber", sn_s.as_str()));
    }
    if let Some(v) = nonempty(s.native_id.as_deref()) {
        tag.push_attribute(("nativeID", v));
    }

    if let Some(v) = nonempty(s.data_processing_ref.as_deref()) {
        tag.push_attribute(("dataProcessingRef", v));
    }

    if let Some(v) = nonempty(s.source_file_ref.as_deref()) {
        tag.push_attribute(("sourceFileRef", v));
    }
    if let Some(v) = nonempty(s.spot_id.as_deref()) {
        tag.push_attribute(("spotID", v));
    }

    let off = write_start_capture_offset(writer, tag)?;
    idx.spectrum.push(IndexOffsetAcc {
        id_ref: id_to_write.to_string(),
        offset: off,
    });

    write_referenceable_param_group_refs(writer, &s.referenceable_param_group_refs)?;
    write_cv_params(writer, &s.cv_params)?;
    write_user_params(writer, &s.user_params)?;

    let (sd_has_scan_list, sd_has_precursor_list, sd_has_product_list) =
        match &s.spectrum_description {
            Some(sd) => (
                sd.scan_list.is_some(),
                sd.precursor_list.is_some(),
                sd.product_list.is_some(),
            ),
            None => (false, false, false),
        };

    if let Some(sd) = &s.spectrum_description {
        write_spectrum_description(writer, sd)?;
    }

    if !sd_has_scan_list && let Some(sl) = &s.scan_list {
        write_scan_list(writer, sl)?;
    }

    if !sd_has_precursor_list && let Some(pl) = &s.precursor_list {
        write_precursor_list(writer, pl)?;
    }

    if !sd_has_product_list && let Some(pr) = &s.product_list {
        write_product_list(writer, pr)?;
    }

    if let Some(bdal) = &s.binary_data_array_list {
        write_binary_data_array_list(writer, bdal, fallback_default_dp)?;
    }

    writer.write_event(Event::End(BytesEnd::new("spectrum")))?;
    Ok(())
}

fn write_spectrum_description(
    writer: &mut Writer<Vec<u8>>,
    sd: &SpectrumDescription,
) -> Result<(), BinToMzmlError> {
    writer.write_event(Event::Start(BytesStart::new("spectrumDescription")))?;

    write_referenceable_param_group_refs(writer, &sd.referenceable_param_group_refs)?;
    write_cv_params(writer, &sd.cv_params)?;
    write_user_params(writer, &sd.user_params)?;

    if let Some(sl) = &sd.scan_list {
        write_scan_list(writer, sl)?;
    }
    if let Some(pl) = &sd.precursor_list {
        write_precursor_list(writer, pl)?;
    }
    if let Some(pr) = &sd.product_list {
        write_product_list(writer, pr)?;
    }

    writer.write_event(Event::End(BytesEnd::new("spectrumDescription")))?;
    Ok(())
}

fn write_scan_list(writer: &mut Writer<Vec<u8>>, list: &ScanList) -> Result<(), BinToMzmlError> {
    let count = list.count.unwrap_or(list.scans.len());
    let mut tag = BytesStart::new("scanList");
    let count_s = count.to_string();
    tag.push_attribute(("count", count_s.as_str()));

    writer.write_event(Event::Start(tag))?;

    write_referenceable_param_group_refs(writer, &list.referenceable_param_group_refs)?;
    write_cv_params(writer, &list.cv_params)?;
    write_user_params(writer, &list.user_params)?;

    for s in &list.scans {
        let mut st = BytesStart::new("scan");
        if let Some(v) = nonempty(s.instrument_configuration_ref.as_deref()) {
            st.push_attribute(("instrumentConfigurationRef", v));
        }
        if let Some(v) = nonempty(s.external_spectrum_id.as_deref()) {
            st.push_attribute(("externalSpectrumID", v));
        }
        if let Some(v) = nonempty(s.source_file_ref.as_deref()) {
            st.push_attribute(("sourceFileRef", v));
        }
        if let Some(v) = nonempty(s.spectrum_ref.as_deref()) {
            st.push_attribute(("spectrumRef", v));
        }

        writer.write_event(Event::Start(st))?;

        write_referenceable_param_group_refs(writer, &s.referenceable_param_group_refs)?;
        write_cv_params(writer, &s.cv_params)?;
        write_user_params(writer, &s.user_params)?;

        if let Some(swl) = &s.scan_window_list {
            write_scan_window_list(writer, swl)?;
        }

        writer.write_event(Event::End(BytesEnd::new("scan")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("scanList")))?;
    Ok(())
}

fn write_scan_window_list(
    writer: &mut Writer<Vec<u8>>,
    list: &ScanWindowList,
) -> Result<(), BinToMzmlError> {
    write_list(
        writer,
        "scanWindowList",
        list.count.unwrap_or(list.scan_windows.len()),
        &list.scan_windows,
        |writer, w| {
            writer.write_event(Event::Start(BytesStart::new("scanWindow")))?;
            write_cv_params(writer, &w.cv_params)?;
            write_user_params(writer, &w.user_params)?;
            writer.write_event(Event::End(BytesEnd::new("scanWindow")))?;
            Ok(())
        },
    )
}

fn write_precursor_list(
    writer: &mut Writer<Vec<u8>>,
    list: &PrecursorList,
) -> Result<(), BinToMzmlError> {
    let count = list.count.unwrap_or(list.precursors.len());
    let mut tag = BytesStart::new("precursorList");
    let count_s = count.to_string();
    tag.push_attribute(("count", count_s.as_str()));

    writer.write_event(Event::Start(tag))?;

    write_cv_params(writer, &list.cv_params)?;
    write_user_params(writer, &list.user_params)?;

    for p in &list.precursors {
        write_precursor(writer, p)?;
    }

    writer.write_event(Event::End(BytesEnd::new("precursorList")))?;
    Ok(())
}

fn write_precursor(writer: &mut Writer<Vec<u8>>, p: &Precursor) -> Result<(), BinToMzmlError> {
    let mut pt = BytesStart::new("precursor");
    if let Some(v) = nonempty(p.spectrum_ref.as_deref()) {
        pt.push_attribute(("spectrumRef", v));
    }
    if let Some(v) = nonempty(p.source_file_ref.as_deref()) {
        pt.push_attribute(("sourceFileRef", v));
    }
    if let Some(v) = nonempty(p.external_spectrum_id.as_deref()) {
        pt.push_attribute(("externalSpectrumID", v));
    }

    writer.write_event(Event::Start(pt))?;

    if let Some(iw) = &p.isolation_window {
        write_cv_container(
            writer,
            "isolationWindow",
            &iw.referenceable_param_group_refs,
            &iw.cv_params,
            &iw.user_params,
        )?;
    }
    if let Some(sil) = &p.selected_ion_list {
        write_selected_ion_list(writer, sil)?;
    }
    if let Some(act) = &p.activation {
        write_cv_container(
            writer,
            "activation",
            &act.referenceable_param_group_refs,
            &act.cv_params,
            &act.user_params,
        )?;
    }

    writer.write_event(Event::End(BytesEnd::new("precursor")))?;
    Ok(())
}

fn write_selected_ion_list(
    writer: &mut Writer<Vec<u8>>,
    list: &SelectedIonList,
) -> Result<(), BinToMzmlError> {
    write_list(
        writer,
        "selectedIonList",
        list.count.unwrap_or(list.selected_ions.len()),
        &list.selected_ions,
        |writer, si| {
            writer.write_event(Event::Start(BytesStart::new("selectedIon")))?;
            write_referenceable_param_group_refs(writer, &si.referenceable_param_group_refs)?;
            write_cv_params(writer, &si.cv_params)?;
            write_user_params(writer, &si.user_params)?;
            writer.write_event(Event::End(BytesEnd::new("selectedIon")))?;
            Ok(())
        },
    )
}

fn write_product_list(
    writer: &mut Writer<Vec<u8>>,
    list: &ProductList,
) -> Result<(), BinToMzmlError> {
    let count = list.count.unwrap_or(list.products.len());
    let mut tag = BytesStart::new("productList");
    let count_s = count.to_string();
    tag.push_attribute(("count", count_s.as_str()));

    writer.write_event(Event::Start(tag))?;

    write_cv_params(writer, &list.cv_params)?;
    write_user_params(writer, &list.user_params)?;

    for p in &list.products {
        write_product(writer, p)?;
    }

    writer.write_event(Event::End(BytesEnd::new("productList")))?;
    Ok(())
}

fn write_product(writer: &mut Writer<Vec<u8>>, p: &Product) -> Result<(), BinToMzmlError> {
    let mut pt = BytesStart::new("product");
    if let Some(v) = nonempty(p.spectrum_ref.as_deref()) {
        pt.push_attribute(("spectrumRef", v));
    }
    if let Some(v) = nonempty(p.source_file_ref.as_deref()) {
        pt.push_attribute(("sourceFileRef", v));
    }
    if let Some(v) = nonempty(p.external_spectrum_id.as_deref()) {
        pt.push_attribute(("externalSpectrumID", v));
    }

    writer.write_event(Event::Start(pt))?;

    write_cv_params(writer, &p.cv_params)?;
    write_user_params(writer, &p.user_params)?;

    if let Some(iw) = &p.isolation_window {
        write_cv_container(
            writer,
            "isolationWindow",
            &iw.referenceable_param_group_refs,
            &iw.cv_params,
            &iw.user_params,
        )?;
    }

    writer.write_event(Event::End(BytesEnd::new("product")))?;
    Ok(())
}

fn write_chromatogram_list(
    writer: &mut Writer<Vec<u8>>,
    list: &ChromatogramList,
    fallback_default_dp: Option<&str>,
    idx: &mut IndexAcc,
) -> Result<(), BinToMzmlError> {
    let count = list.count.unwrap_or(list.chromatograms.len());
    let mut tag = BytesStart::new("chromatogramList");
    let count_s = count.to_string();
    tag.push_attribute(("count", count_s.as_str()));

    if let Some(dp) = nonempty(list.default_data_processing_ref.as_deref()) {
        tag.push_attribute(("defaultDataProcessingRef", dp));
    }

    writer.write_event(Event::Start(tag))?;

    for c in &list.chromatograms {
        write_chromatogram(writer, c, fallback_default_dp, idx)?;
    }

    writer.write_event(Event::End(BytesEnd::new("chromatogramList")))?;
    Ok(())
}

fn write_chromatogram(
    writer: &mut Writer<Vec<u8>>,
    c: &Chromatogram,
    fallback_default_dp: Option<&str>,
    idx: &mut IndexAcc,
) -> Result<(), BinToMzmlError> {
    let mut tag = BytesStart::new("chromatogram");
    tag.push_attribute(("id", c.id.as_str()));
    if let Some(v) = nonempty(c.native_id.as_deref()) {
        tag.push_attribute(("nativeID", v));
    }
    if let Some(idx0) = c.index {
        let idx_s = idx0.to_string();
        tag.push_attribute(("index", idx_s.as_str()));
    }

    if let Some(default_len) = c.default_array_length {
        let len_s = default_len.to_string();
        tag.push_attribute(("defaultArrayLength", len_s.as_str()));
    }

    if let Some(v) = nonempty(c.data_processing_ref.as_deref()) {
        tag.push_attribute(("dataProcessingRef", v));
    }

    let off = write_start_capture_offset(writer, tag)?;
    idx.chromatogram.push(IndexOffsetAcc {
        id_ref: c.id.clone(),
        offset: off,
    });

    write_referenceable_param_group_refs(writer, &c.referenceable_param_group_refs)?;
    write_cv_params(writer, &c.cv_params)?;
    write_user_params(writer, &c.user_params)?;

    if let Some(p) = &c.precursor {
        write_precursor(writer, p)?;
    }
    if let Some(p) = &c.product {
        write_product(writer, p)?;
    }

    if let Some(bdal) = &c.binary_data_array_list {
        write_binary_data_array_list(writer, bdal, fallback_default_dp)?;
    }

    writer.write_event(Event::End(BytesEnd::new("chromatogram")))?;
    Ok(())
}

fn write_binary_data_array_list(
    writer: &mut Writer<Vec<u8>>,
    list: &BinaryDataArrayList,
    fallback_default_dp: Option<&str>,
) -> Result<(), BinToMzmlError> {
    let count = list.count.unwrap_or(list.binary_data_arrays.len());
    let mut tag = BytesStart::new("binaryDataArrayList");
    let count_s = count.to_string();
    tag.push_attribute(("count", count_s.as_str()));

    writer.write_event(Event::Start(tag))?;

    for bda in &list.binary_data_arrays {
        write_binary_data_array(writer, bda, fallback_default_dp)?;
    }

    writer.write_event(Event::End(BytesEnd::new("binaryDataArrayList")))?;
    Ok(())
}

fn write_binary_data_array(
    writer: &mut Writer<Vec<u8>>,
    bda: &BinaryDataArray,
    _fallback_default_dp: Option<&str>,
) -> Result<(), BinToMzmlError> {
    let has_accession = |acc: &str| {
        bda.cv_params
            .iter()
            .any(|p| p.accession.as_deref() == Some(acc))
    };

    let cv_has_zlib = has_accession(ACC_COMPRESSION_ZLIB);
    let cv_has_no_comp = has_accession(ACC_COMPRESSION_NONE);
    let synthesize_no_compression = !cv_has_zlib && !cv_has_no_comp;

    let cv_has_f64 = has_accession(ACC_FLOAT_64BIT_STR);
    let cv_has_f32 = has_accession(ACC_FLOAT_32BIT_STR);
    let cv_has_i64 = has_accession(ACC_INT_64BIT_STR);
    let cv_has_i32 = has_accession(ACC_INT_32BIT_STR);
    let cv_has_i16 = has_accession(ACC_INT_16BIT_STR);

    let encoded = if let Some(binary) = bda.binary.as_ref() {
        if let Some(nt) = bda.numeric_type {
            let ok = matches!(
                (binary, nt),
                (NumericArray::F64(_), NumericType::Float64)
                    | (NumericArray::F32(_), NumericType::Float32)
                    | (NumericArray::F16(_), NumericType::Float16)
                    | (NumericArray::I64(_), NumericType::Int64)
                    | (NumericArray::I32(_), NumericType::Int32)
                    | (NumericArray::I16(_), NumericType::Int16)
            );
            if !ok {
                return Err(BinToMzmlError::InvalidData("binary/numeric_type mismatch"));
            }
        }

        let (mut raw_bytes, inferred_numeric_type) = match binary {
            NumericArray::F64(v) => {
                let mut bytes = Vec::with_capacity(v.len() * 8);
                for &x in v {
                    bytes.extend_from_slice(&x.to_le_bytes());
                }
                (bytes, NumericType::Float64)
            }
            NumericArray::F32(v) => {
                let mut bytes = Vec::with_capacity(v.len() * 4);
                for &x in v {
                    bytes.extend_from_slice(&x.to_le_bytes());
                }
                (bytes, NumericType::Float32)
            }
            NumericArray::F16(v) => {
                let mut bytes = Vec::with_capacity(v.len() * 2);
                for &x in v {
                    bytes.extend_from_slice(&x.to_le_bytes());
                }
                (bytes, NumericType::Float16)
            }
            NumericArray::I64(v) => {
                let mut bytes = Vec::with_capacity(v.len() * 8);
                for &x in v {
                    bytes.extend_from_slice(&x.to_le_bytes());
                }
                (bytes, NumericType::Int64)
            }
            NumericArray::I32(v) => {
                let mut bytes = Vec::with_capacity(v.len() * 4);
                for &x in v {
                    bytes.extend_from_slice(&x.to_le_bytes());
                }
                (bytes, NumericType::Int32)
            }
            NumericArray::I16(v) => {
                let mut bytes = Vec::with_capacity(v.len() * 2);
                for &x in v {
                    bytes.extend_from_slice(&x.to_le_bytes());
                }
                (bytes, NumericType::Int16)
            }
        };

        if cv_has_zlib && !raw_bytes.is_empty() {
            raw_bytes = compress_to_vec_zlib(&raw_bytes, 6);
        }

        match inferred_numeric_type {
            NumericType::Float64
                if !(cv_has_f64 || bda.numeric_type == Some(NumericType::Float64)) =>
            {
                return Err(BinToMzmlError::InvalidData(
                    "binaryDataArray F64 but missing cvParam MS:1000523",
                ));
            }
            NumericType::Float32
                if !(cv_has_f32 || bda.numeric_type == Some(NumericType::Float32)) =>
            {
                return Err(BinToMzmlError::InvalidData(
                    "binaryDataArray F32 but missing cvParam MS:1000521",
                ));
            }
            NumericType::Float16
                if !(has_accession(ACC_FLOAT_16BIT_STR)
                    || bda.numeric_type == Some(NumericType::Float16)) =>
            {
                return Err(BinToMzmlError::InvalidData(
                    "binaryDataArray F16 but missing cvParam MS:1000520",
                ));
            }
            NumericType::Int64 if !(cv_has_i64 || bda.numeric_type == Some(NumericType::Int64)) => {
                return Err(BinToMzmlError::InvalidData(
                    "binaryDataArray I64 but missing cvParam MS:1000522",
                ));
            }
            NumericType::Int32 if !(cv_has_i32 || bda.numeric_type == Some(NumericType::Int32)) => {
                return Err(BinToMzmlError::InvalidData(
                    "binaryDataArray I32 but missing cvParam MS:1000519",
                ));
            }
            NumericType::Int16 if !(cv_has_i16 || bda.numeric_type == Some(NumericType::Int16)) => {
                return Err(BinToMzmlError::InvalidData(
                    "binaryDataArray I16 but missing cvParam MS:1000518",
                ));
            }
            _ => {}
        }

        if raw_bytes.is_empty() {
            String::new()
        } else {
            STANDARD.encode(&raw_bytes)
        }
    } else {
        String::new()
    };

    let mut tag = BytesStart::new("binaryDataArray");

    if let Some(al) = bda.array_length
        && al > 0
    {
        let al_s = al.to_string();
        tag.push_attribute(("arrayLength", al_s.as_str()));
    }

    let el = encoded.len();
    let el_s = el.to_string();
    tag.push_attribute(("encodedLength", el_s.as_str()));

    if let Some(dp) = nonempty(bda.data_processing_ref.as_deref()) {
        tag.push_attribute(("dataProcessingRef", dp));
    }

    writer.write_event(Event::Start(tag))?;

    for r in &bda.referenceable_param_group_refs {
        let mut t = BytesStart::new("referenceableParamGroupRef");
        t.push_attribute(("ref", r.r#ref.as_str()));
        writer.write_event(Event::Empty(t))?;
    }

    write_cv_params(writer, &bda.cv_params)?;
    if synthesize_no_compression {
        write_cv_param(
            writer,
            &CvParam {
                cv_ref: Some("MS".to_string()),
                accession: Some(ACC_COMPRESSION_NONE.to_string()),
                name: "no compression".to_string(),
                ..Default::default()
            },
        )?;
    }
    write_user_params(writer, &bda.user_params)?;

    writer.write_event(Event::Start(BytesStart::new("binary")))?;
    if !encoded.is_empty() {
        writer.write_event(Event::Text(BytesText::new(encoded.as_str())))?;
    }
    writer.write_event(Event::End(BytesEnd::new("binary")))?;

    writer.write_event(Event::End(BytesEnd::new("binaryDataArray")))?;

    Ok(())
}

fn write_target_list(
    writer: &mut Writer<Vec<u8>>,
    list: &TargetList,
) -> Result<(), BinToMzmlError> {
    write_list(
        writer,
        "targetList",
        list.count.unwrap_or(list.targets.len()),
        &list.targets,
        |writer, t| {
            writer.write_event(Event::Start(BytesStart::new("target")))?;
            write_referenceable_param_group_refs(writer, &t.referenceable_param_group_refs)?;
            write_cv_params(writer, &t.cv_params)?;
            write_user_params(writer, &t.user_params)?;
            writer.write_event(Event::End(BytesEnd::new("target")))?;
            Ok(())
        },
    )
}

fn write_source_file_ref_list(
    writer: &mut Writer<Vec<u8>>,
    list: &SourceFileRefList,
) -> Result<(), BinToMzmlError> {
    write_list(
        writer,
        "sourceFileRefList",
        list.count.unwrap_or(list.source_file_refs.len()),
        &list.source_file_refs,
        |writer, r| {
            let mut rf = BytesStart::new("sourceFileRef");
            rf.push_attribute(("ref", r.r#ref.as_str()));
            writer.write_event(Event::Empty(rf))?;
            Ok(())
        },
    )
}

#[inline]
fn write_referenceable_param_group_refs(
    writer: &mut Writer<Vec<u8>>,
    refs: &[ReferenceableParamGroupRef],
) -> Result<(), BinToMzmlError> {
    for r in refs {
        write_referenceable_param_group_ref(writer, r)?;
    }
    Ok(())
}

#[inline]
fn write_referenceable_param_group_ref(
    writer: &mut Writer<Vec<u8>>,
    r: &ReferenceableParamGroupRef,
) -> Result<(), BinToMzmlError> {
    let mut tag = BytesStart::new("referenceableParamGroupRef");
    tag.push_attribute(("ref", r.r#ref.as_str()));
    writer
        .write_event(Event::Empty(tag))
        .map_err(BinToMzmlError::from)
}

#[inline]
fn write_cv_param(writer: &mut Writer<Vec<u8>>, cv: &CvParam) -> Result<(), BinToMzmlError> {
    let mut tag = BytesStart::new("cvParam");

    if let Some(v) = cv.cv_ref.as_deref().and_then(|s| nonempty(Some(s))) {
        tag.push_attribute(("cvRef", v));
    }
    if let Some(v) = cv.accession.as_deref().and_then(|s| nonempty(Some(s))) {
        tag.push_attribute(("accession", v));
    }
    tag.push_attribute(("name", cv.name.as_str()));

    if let Some(v) = cv.value.as_deref() {
        tag.push_attribute(("value", v));
    }

    if let Some(v) = cv.unit_cv_ref.as_deref().and_then(|s| nonempty(Some(s))) {
        tag.push_attribute(("unitCvRef", v));
    }
    if let Some(v) = cv.unit_accession.as_deref().and_then(|s| nonempty(Some(s))) {
        tag.push_attribute(("unitAccession", v));
    }
    if let Some(v) = cv.unit_name.as_deref().and_then(|s| nonempty(Some(s))) {
        tag.push_attribute(("unitName", v));
    }

    writer
        .write_event(Event::Empty(tag))
        .map_err(BinToMzmlError::from)
}

fn write_cv_params(writer: &mut Writer<Vec<u8>>, params: &[CvParam]) -> Result<(), BinToMzmlError> {
    for cv in params {
        write_cv_param(writer, cv)?;
    }
    Ok(())
}

#[inline]
fn write_user_param(writer: &mut Writer<Vec<u8>>, up: &UserParam) -> Result<(), BinToMzmlError> {
    let mut tag = BytesStart::new("userParam");
    tag.push_attribute(("name", up.name.as_str()));

    if let Some(v) = up.r#type.as_deref().and_then(|s| nonempty(Some(s))) {
        tag.push_attribute(("type", v));
    }

    if let Some(v) = up.value.as_deref() {
        tag.push_attribute(("value", v));
    }

    if let Some(v) = up.unit_cv_ref.as_deref().and_then(|s| nonempty(Some(s))) {
        tag.push_attribute(("unitCvRef", v));
    }
    if let Some(v) = up.unit_accession.as_deref().and_then(|s| nonempty(Some(s))) {
        tag.push_attribute(("unitAccession", v));
    }
    if let Some(v) = up.unit_name.as_deref().and_then(|s| nonempty(Some(s))) {
        tag.push_attribute(("unitName", v));
    }

    writer
        .write_event(Event::Empty(tag))
        .map_err(BinToMzmlError::from)
}

fn write_user_params(
    writer: &mut Writer<Vec<u8>>,
    params: &[UserParam],
) -> Result<(), BinToMzmlError> {
    for up in params {
        write_user_param(writer, up)?;
    }
    Ok(())
}

fn write_cv_container(
    writer: &mut Writer<Vec<u8>>,
    tag_name: &str,
    refs: &[ReferenceableParamGroupRef],
    cvs: &[CvParam],
    ups: &[UserParam],
) -> Result<(), BinToMzmlError> {
    writer.write_event(Event::Start(BytesStart::new(tag_name)))?;

    write_referenceable_param_group_refs(writer, refs)?;
    write_cv_params(writer, cvs)?;
    write_user_params(writer, ups)?;

    writer.write_event(Event::End(BytesEnd::new(tag_name)))?;
    Ok(())
}

fn write_index_list_with_offset(
    writer: &mut Writer<Vec<u8>>,
    idx: &IndexAcc,
) -> Result<u64, BinToMzmlError> {
    let mut count = 0usize;
    if !idx.spectrum.is_empty() {
        count += 1;
    }
    if !idx.chromatogram.is_empty() {
        count += 1;
    }

    let mut tag = BytesStart::new("indexList");
    let count_s = count.to_string();
    tag.push_attribute(("count", count_s.as_str()));

    let off = write_start_capture_offset(writer, tag)?;

    if !idx.spectrum.is_empty() {
        write_index(writer, "spectrum", &idx.spectrum)?;
    }
    if !idx.chromatogram.is_empty() {
        write_index(writer, "chromatogram", &idx.chromatogram)?;
    }

    writer.write_event(Event::End(BytesEnd::new("indexList")))?;

    Ok(off)
}

fn write_index(
    writer: &mut Writer<Vec<u8>>,
    name: &str,
    offsets: &Vec<IndexOffsetAcc>,
) -> Result<(), BinToMzmlError> {
    let mut tag = BytesStart::new("index");
    tag.push_attribute(("name", name));

    writer.write_event(Event::Start(tag))?;

    for o in offsets {
        let mut ot = BytesStart::new("offset");
        ot.push_attribute(("idRef", o.id_ref.as_str()));

        writer.write_event(Event::Start(ot))?;

        let s = o.offset.to_string();
        writer.write_event(Event::Text(BytesText::new(s.as_str())))?;

        writer.write_event(Event::End(BytesEnd::new("offset")))?;
    }

    writer.write_event(Event::End(BytesEnd::new("index")))?;
    Ok(())
}

fn write_index_list_offset(writer: &mut Writer<Vec<u8>>, off: u64) -> Result<(), BinToMzmlError> {
    writer.write_event(Event::Start(BytesStart::new("indexListOffset")))?;

    let s = off.to_string();
    writer.write_event(Event::Text(BytesText::new(s.as_str())))?;

    writer.write_event(Event::End(BytesEnd::new("indexListOffset")))?;
    Ok(())
}

fn write_file_checksum(writer: &mut Writer<Vec<u8>>) -> Result<(), BinToMzmlError> {
    writer.write_event(Event::Start(BytesStart::new("fileChecksum")))?;

    let digest = Sha1::digest(writer.get_ref());
    let hex: String = digest.iter().fold(String::with_capacity(40), |mut acc, b| {
        use std::fmt::Write;
        let _ = write!(acc, "{b:02x}");
        acc
    });

    writer.write_event(Event::Text(BytesText::new(&hex)))?;

    writer.write_event(Event::End(BytesEnd::new("fileChecksum")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_cv_param(cv: &CvParam) -> String {
        let mut writer = Writer::new(Vec::new());
        write_cv_param(&mut writer, cv).unwrap();
        String::from_utf8(writer.into_inner()).unwrap()
    }

    #[test]
    fn valueless_cvparam_omits_value_attribute_7() {
        let valueless = CvParam {
            cv_ref: Some("MS".to_string()),
            accession: Some("MS:1000511".to_string()),
            name: "ms level".to_string(),
            value: None,
            ..Default::default()
        };
        let xml = render_cv_param(&valueless);
        assert!(
            !xml.contains("value="),
            "a valueless cvParam must not emit a value attribute, got: {xml}"
        );

        let with_value = CvParam {
            value: Some("x".to_string()),
            ..valueless
        };
        let xml = render_cv_param(&with_value);
        assert!(
            xml.contains(r#"value="x""#),
            "a cvParam with a value must emit it verbatim, got: {xml}"
        );
    }
}
