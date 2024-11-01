use std::path::Path;
use charming::{component::{Grid, Axis}, Chart, ImageRenderer};
use charming::element::{AxisLabel, AxisType, Formatter};
use charming::element::LabelPosition;
use charming::element::Label;
use charming::series::{Bar, Series};
use charming::component::{Legend};
use charming::component::Title;
use str_macro::str;
use std::string::String;
use crate::{read_group, EstimatesMPMC};
use crate::CHART_WIDTH;

pub fn mpmc(dir_name: impl AsRef<Path>) {
    let dir_name = dir_name.as_ref();
    
    let wts = [1,2,4,8];
    let rts = [1,2,4,8];
    
    let chute_spmc_w_mutex = read_group(
        &std::path::Path::new(dir_name).join("chute__spmc_mutex")
        ,&wts, &rts
    );
    
    let chute_mpmc = read_group(
        &std::path::Path::new(dir_name).join("chute__mpmc")
        ,&wts, &rts
    );
    
    let tokio_broadcast = read_group(
        &std::path::Path::new(dir_name).join("tokio__broadcast")
        ,&wts, &rts
    );
    
    let all: Vec<(String, EstimatesMPMC)> = vec![
        (str!("chute::spmc\nw/ mutex"), chute_spmc_w_mutex),
        (str!("chute::mpmc"), chute_mpmc),
        (str!("tokio::broadcast"), tokio_broadcast),
    ];
    
    chart(&all, 4, str!("mpmc (4 readers)"), "out/mpmc.svg");    
}

/// `rt` - read thread count
pub fn chart(
    all_estimates: &Vec<(String, EstimatesMPMC)>, 
    rt: usize, 
    title: String,
    fname: impl AsRef<std::path::Path>
) {
    let wts: Vec<usize> = all_estimates.first().unwrap().1
        .iter().map(|(wt, _)| *wt)
        .collect();
    
    let unit = String::from("ms");
    let ns_to_unit = 1.0 / 1_000_000.0;
    
    let mut chart = 
    Chart::new()
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
        )
        .y_axis(
            Axis::new()
                .type_(AxisType::Category)
                .data(
                    all_estimates.iter()
                    .map(|(name,_)| name.clone())
                    .collect()
                ),
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
    
    for wt in wts {
        let mut bar = 
            Bar::new()
            .name(format!("{wt} writers"))
            .label(
                Label::new()
                .show(true)
                .position(LabelPosition::InsideRight)
                .formatter(Formatter::Function(
                    (
                        "function (param) { return param.data.toFixed(2) + \"".to_string()
                        + &unit
                        + "\"; }"
                    ).into()
                ))
            );
        let mut datas = Vec::new();
        for (_, estimates) in all_estimates {
            let data_ns = estimates[&wt][&rt];
            datas.push(data_ns * ns_to_unit);
        }
        bar = bar.data(datas);
        chart = chart.series(Series::Bar(bar));
    }
    
    let mut renderer = ImageRenderer::new(CHART_WIDTH, 400);
    renderer.save(&chart, fname).unwrap();
    //renderer.save_format(ImageFormat::Png, &chart, "charming.png");    
}