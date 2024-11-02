use std::path::Path;
use charming::{Chart, ImageRenderer};
use charming::component::{Axis, Grid, Legend, Title};
use charming::element::{AxisLabel, AxisType, Formatter, Label, LabelPosition};
use charming::element::font_settings::FontWeight;
use charming::series::{Bar, Series};
use charming::theme::Theme;
use str_macro::str;
use crate::{read_estimate};
use crate::CHART_WIDTH;
use crate::CHART_THEME;
use crate::CHART_BACKGROUND;

pub fn spsc(dir_name: impl AsRef<Path>) {
    let dir_name = dir_name.as_ref();
    
    let chute_spmc = read_estimate(
        &std::path::Path::new(dir_name).join("chute__spmc")
    );
    
    let chute_mpmc = read_estimate(
        &std::path::Path::new(dir_name).join("chute__mpmc")
    );
    
    let crossbeam_unbounded = read_estimate(
        &std::path::Path::new(dir_name).join("crossbeam__unbounded")
    );
    
    let all: Vec<(String, f64)> = vec![
        (str!("chute::spmc"), chute_spmc),
        (str!("chute::mpmc"), chute_mpmc),
        (str!("crossbeam::\nunbounded")/*str!("crossbeam")*/, crossbeam_unbounded),
    ];
    
    chart(&all, str!("spsc"), "out/spsc");    
}

pub fn chart(
    all_estimates: &Vec<(String, f64)>,
    title: String,
    fname: impl AsRef<std::path::Path>,
){
    let unit = String::from("ms");
    let ns_to_unit = 1.0 / 1_000_000.0;
    
    let mut chart = 
    Chart::new()
        .background_color(CHART_BACKGROUND)
        .title(
            Title::new()
            .text(title)
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
                .bottom(25)
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
                    AxisLabel::new()
                        .show(true)
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
    
    {
        let mut bar = 
            Bar::new()
            .bar_width(24)
            //.name(format!("{wt} writers"))
            .label(
                Label::new()
                .show(true)
                .position(LabelPosition::Right)
                .formatter(Formatter::Function(
                    (
                        "function (param) { return param.data.toFixed(2) + \"".to_string()
                        //+ &unit
                        + "\"; }"
                    ).into()
                ))
            );
        let mut datas = Vec::new();
        for (_, estimate) in all_estimates {
            let data_ns = estimate;
            datas.push(data_ns * ns_to_unit);
        }
        bar = bar.data(datas);
        chart = chart.series(Series::Bar(bar));
    }
    
    let mut renderer = ImageRenderer::new(CHART_WIDTH, 180).theme(CHART_THEME);
    renderer.save(&chart, fname.as_ref().with_extension("svg")).unwrap();
    renderer.save_format(charming::ImageFormat::Png, &chart, fname.as_ref().with_extension("png")).unwrap();    
}