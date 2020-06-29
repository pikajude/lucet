use super::bench::AnonymousTestResult;
use super::config::OutputConfig;
use super::errors::*;
use bencher::stats::Summary;
use printtable;
use serde::ser::{Serialize, SerializeMap, Serializer};
use serde_json;
use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::io::Write;

pub struct Text;
pub struct CSV;
pub struct JSON;

#[derive(Copy, Clone, Debug)]
pub enum Format {
    Text,
    CSV,
    JSON,
}

impl Format {
    pub fn parse(format_str: &str) -> Result<Format, BenchError> {
        match format_str {
            "Text" => Ok(Format::Text),
            "CSV" => Ok(Format::CSV),
            "JSON" => Ok(Format::JSON),
            _ => Err(BenchError::Unsupported),
        }
    }
}

pub trait Serializable<W: Write> {
    fn out(
        &self,
        writer: W,
        test_suites_results: &HashMap<String, HashMap<String, AnonymousTestResult>>,
        breakdown: bool,
    ) -> Result<(), BenchError>;
}

pub struct Out {
    test_suites_results: HashMap<String, HashMap<String, AnonymousTestResult>>,
}

impl Out {
    pub fn new(test_suites_results: HashMap<String, HashMap<String, AnonymousTestResult>>) -> Out {
        Out {
            test_suites_results,
        }
    }

    pub fn out_vec(&self, output_configs: &[OutputConfig]) -> Result<(), BenchError> {
        for output_config in output_configs {
            let format = Format::parse(&output_config.format)?;
            let writer: Box<dyn Write> = match output_config.file {
                Some(ref file) if !file.is_empty() => Box::new(File::create(file)?),
                _ => Box::new(io::stdout()),
            };
            self.out(writer, format, output_config.breakdown.unwrap_or(false))?;
        }
        Ok(())
    }

    pub fn out<W: Write>(
        &self,
        writer: W,
        format: Format,
        breakdown: bool,
    ) -> Result<(), BenchError> {
        let serializer: Box<dyn Serializable<W>> = match format {
            Format::Text => Box::new(Text) as Box<_>,
            Format::CSV => Box::new(CSV) as Box<_>,
            Format::JSON => Box::new(JSON) as Box<_>,
        };
        serializer.out(writer, &self.test_suites_results, breakdown)
    }
}

fn into_sorted(
    test_suites_results: &HashMap<String, HashMap<String, AnonymousTestResult>>,
) -> Vec<(&String, Vec<(&String, &AnonymousTestResult)>)> {
    let mut test_suites_results: Vec<_> = test_suites_results
        .iter()
        .map(|(test_name, test_suite)| {
            let mut test_suite: Vec<_> = test_suite.iter().collect();
            test_suite.sort_unstable_by_key(|x| x.0);
            (test_name, test_suite)
        })
        .collect();
    test_suites_results.sort_unstable_by_key(|x| x.0);
    test_suites_results
}

/// Text output
impl<W: Write> Serializable<W> for Text {
    fn out(
        &self,
        writer: W,
        test_suites_results: &HashMap<String, HashMap<String, AnonymousTestResult>>,
        breakdown: bool,
    ) -> Result<(), BenchError> {
        let mut header = vec!["Test", "Implementation", "Ratio", "Median", "RSD"];
        if breakdown {
            header.push("Function");
            header.push("Percentage");
        }
        let mut mat = vec![];
        for (test_name, test_suite) in into_sorted(test_suites_results) {
            let mut ref_mean = None;
            for (test_suite_name, anonymous_test_result) in test_suite {
                ref_mean = ref_mean.or_else(|| Some(anonymous_test_result.grand_summary.mean));
                let ratio = match ref_mean {
                    Some(ref_mean) if ref_mean > 0.0 => {
                        anonymous_test_result.grand_summary.mean / ref_mean
                    }
                    _ => 0.0,
                };
                let rsd = match anonymous_test_result.grand_summary.mean {
                    mean if mean > 0.0 => {
                        anonymous_test_result.grand_summary.std_dev * 100.0 / mean
                    }
                    _ => 0.0,
                };
                let ratio = format!("{}", ratio);
                let median = format!("{}", anonymous_test_result.grand_summary.median);
                let rsd = format!("{}", rsd);
                let mut line = vec![
                    test_name.to_owned(),
                    test_suite_name.to_owned(),
                    ratio,
                    median,
                    rsd,
                ];
                if breakdown {
                    line.push("".to_owned());
                    line.push("".to_owned());
                }
                mat.push(line);

                let bodies_median_sum = anonymous_test_result
                    .bodies_summary
                    .iter()
                    .map(|body_summary| body_summary.summary.median)
                    .sum::<f64>();
                let include_breakdown =
                    bodies_median_sum > 0.0 && anonymous_test_result.bodies_summary.len() > 1;
                if include_breakdown {
                    for body_summary in &anonymous_test_result.bodies_summary {
                        let name = &body_summary.name;
                        let pct = body_summary.summary.median * 100.0 / bodies_median_sum;
                        let line = vec![
                            "".to_owned(),
                            "".to_owned(),
                            "".to_owned(),
                            "".to_owned(),
                            "".to_owned(),
                            name.to_owned(),
                            format!("{:.2} %", pct),
                        ];
                        mat.push(line);
                    }
                }
            }
        }
        printtable::write(writer, header, mat).map_err(BenchError::Io)
    }
}

/// CSV output
impl<W: Write> Serializable<W> for CSV {
    fn out(
        &self,
        mut writer: W,
        test_suites_results: &HashMap<String, HashMap<String, AnonymousTestResult>>,
        breakdown: bool,
    ) -> Result<(), BenchError> {
        if breakdown {
            writer.write_all(b"Test\tFunction\tImplementation\tMedian\tRSD\tPercentage\n")?;
            for (test_name, test_suite) in into_sorted(test_suites_results) {
                for (test_suite_name, anonymous_test_result) in test_suite {
                    let bodies_median_sum = anonymous_test_result
                        .bodies_summary
                        .iter()
                        .map(|body_summary| body_summary.summary.median)
                        .sum::<f64>();
                    for body_summary in &anonymous_test_result.bodies_summary {
                        let rsd = match body_summary.summary.mean {
                            mean if mean > 0.0 => body_summary.summary.std_dev * 100.0 / mean,
                            _ => 0.0,
                        };
                        let pct = match bodies_median_sum {
                            sum if sum > 0.0 => body_summary.summary.median * 100.0 / sum,
                            _ => 0.0,
                        };
                        writer.write_all(
                            format!(
                                "{}\t{}\t{}\t{}\t{}\t{:.2} %\n",
                                test_name,
                                body_summary.name,
                                test_suite_name,
                                body_summary.summary.median,
                                rsd,
                                pct
                            )
                            .as_bytes(),
                        )?;
                    }
                }
            }
        } else {
            writer.write_all(b"Test\tImplementation\tRatio\tMedian\tRSD\n")?;
            for (test_name, test_suite) in into_sorted(test_suites_results) {
                let mut ref_mean = None;
                for (test_suite_name, anonymous_test_result) in test_suite {
                    ref_mean = ref_mean.or_else(|| Some(anonymous_test_result.grand_summary.mean));
                    let ratio = match ref_mean {
                        Some(ref_mean) if ref_mean > 0.0 => {
                            anonymous_test_result.grand_summary.mean / ref_mean
                        }
                        _ => 0.0,
                    };
                    let rsd = match anonymous_test_result.grand_summary.mean {
                        mean if mean > 0.0 => {
                            anonymous_test_result.grand_summary.std_dev * 100.0 / mean
                        }
                        _ => 0.0,
                    };
                    writer.write_all(
                        format!(
                            "{}\t{}\t{}\t{}\t{}\n",
                            test_name,
                            test_suite_name,
                            ratio,
                            anonymous_test_result.grand_summary.median,
                            rsd
                        )
                        .as_bytes(),
                    )?;
                }
            }
        }
        Ok(())
    }
}

struct JSONSummary(Summary);

impl Serialize for JSONSummary {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(5))?;
        map.serialize_entry("mean", &(self.0).mean)?;
        map.serialize_entry("median", &(self.0).median)?;
        map.serialize_entry("min", &(self.0).min)?;
        map.serialize_entry("max", &(self.0).max)?;
        map.serialize_entry("std_dev", &(self.0).std_dev)?;
        map.end()
    }
}

impl Serialize for AnonymousTestResult {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let include_breakdown = self.bodies_summary.len() > 1;
        let map_len = if include_breakdown { 6 } else { 5 };
        let mut map = serializer.serialize_map(Some(map_len))?;
        map.serialize_entry("mean", &self.grand_summary.mean)?;
        map.serialize_entry("median", &self.grand_summary.median)?;
        map.serialize_entry("min", &self.grand_summary.min)?;
        map.serialize_entry("max", &self.grand_summary.max)?;
        map.serialize_entry("std_dev", &self.grand_summary.std_dev)?;
        if include_breakdown {
            let json_bodies_summary: Vec<_> = self
                .bodies_summary
                .iter()
                .cloned()
                .map(|body_summary| (body_summary.name, JSONSummary(body_summary.summary)))
                .collect();
            map.serialize_entry("breakdown", &json_bodies_summary)?;
        }
        map.end()
    }
}

#[derive(Default, Serialize)]
struct JSONOutput(Vec<(String, Vec<(String, AnonymousTestResult)>)>);

/// JSON output
impl<W: Write> Serializable<W> for JSON {
    fn out(
        &self,
        mut writer: W,
        test_suites_results: &HashMap<String, HashMap<String, AnonymousTestResult>>,
        _breakdown: bool,
    ) -> Result<(), BenchError> {
        let results: Vec<_> = into_sorted(test_suites_results)
            .into_iter()
            .map(|(test_name, test_suite)| {
                let test_suite: Vec<_> = test_suite
                    .into_iter()
                    .map(|(test_suite_name, anonymous_test_result)| {
                        (test_suite_name, anonymous_test_result)
                    })
                    .collect();
                (test_name, test_suite)
            })
            .collect();
        let json_output_str = serde_json::to_string_pretty(&results)
            .map_err(|e| BenchError::ParseError(e.to_string()))?;
        writer.write_all(json_output_str.as_bytes())?;
        Ok(())
    }
}
