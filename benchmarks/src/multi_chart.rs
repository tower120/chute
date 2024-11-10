use std::collections::BTreeMap;
use std::path::Path;
use charming::{Chart, ImageRenderer};
use charming::component::{Axis, Grid, Legend, Title};
use charming::element::{AxisLabel, AxisType, Formatter, Label, LabelPosition};
use charming::element::font_settings::{FontFamily, FontWeight};
use charming::series::{Bar, Series};
use crate::{CHART_BACKGROUND, CHART_THEME, CHART_WIDTH};

pub struct Visual {
    pub title: String,
    pub label_pos: LabelPosition,
    pub sub_chart_name: String,         // This can be HashMap as well.
}
/*impl Default for Visual {
    fn default() -> Self {
        Visual{
            title: String::new(),
            label_pos: LabelPosition::InsideRight,
            sub_chart_name: 
        }
    }
}*/

pub type MultiChartData = Vec<(String, BTreeMap<usize, f64>)>; 

pub fn multi_chart(
    all_estimates: &MultiChartData, 
    fname: impl AsRef<Path>,
    visual: Visual,
) {
    let sub_chart_ids: Vec<usize> = all_estimates.first().unwrap().1
        .iter().map(|(id, _)| *id)
        .collect();
    
    let unit = String::from("ms");
    let ns_to_unit = 1.0 / 1_000_000.0;
    
    let mut chart = 
    Chart::new()
        .background_color(CHART_BACKGROUND)
        .title(
            Title::new()
            .text(visual.title)
            .left("center")
        )
        .legend(
            Legend::new().top("bottom")
        )
        .grid(
            Grid::new()
                .left(100)
                .right(40)
                .top(40)
                .bottom(60)
        )
        .y_axis(
            Axis::new()
                .type_(AxisType::Category)
                .data(
                    all_estimates.iter()
                    .map(|(name,_)| name.clone())
                    .collect()
                )
                .axis_label(
                    AxisLabel::new().show(true)
                        .font_size(13)
                        .font_weight(FontWeight::Bolder)
                        .color("#666666")
                )
        )    
        .x_axis(
            Axis::new()
                .type_(AxisType::Value)
                .axis_label(AxisLabel::new().formatter(
                    Formatter::String(
                        "{value}".to_string() + &unit
                    )
                ))
        );
    
    // Sub charts
    for sub_chart_id in sub_chart_ids {
        let mut bar = 
            Bar::new()
            .name(format!("{sub_chart_id} {:}", visual.sub_chart_name))
            .label(
                Label::new()
                .show(true)
                .font_size(11)
                .font_weight(FontWeight::Bold)
                .font_family(FontFamily::MonoSpace)
                .position(visual.label_pos.clone())
                .formatter(Formatter::Function(
                    (
                        "function (param) { return param.data.toFixed(2); }"
                    ).into()
                ))
            );
        let mut datas = Vec::new();
        for (_, estimates) in all_estimates {
            let data_ns = estimates[&sub_chart_id];
            datas.push(data_ns * ns_to_unit);
        }
        bar = bar.data(datas);
        chart = chart.series(Series::Bar(bar));
    }
    
    let height = all_estimates.len() as u32 * 80 + 100;
    let mut renderer = ImageRenderer::new(CHART_WIDTH, height).theme(CHART_THEME);
    renderer.save(&chart, fname.as_ref().with_extension("svg")).unwrap();
    renderer.save_format(charming::ImageFormat::Png, &chart, fname.as_ref().with_extension("png")).unwrap();    
}