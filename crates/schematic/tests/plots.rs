use schematic::Interpreter;

fn eval(src: &str) -> String {
    match Interpreter::new().eval_str(src) {
        Ok(schematic::Value::Str(s)) => s.to_string(),
        Ok(v) => v.to_string(),
        Err(e) => format!("error: {e}"),
    }
}

const SHOP: &str = "(define s (table {:city [\"Oslo\" \"Rome\" \"Bern\"] :units [12 7 30] :target [15 10 20] :share [0.2 0.1 0.7]}))";
const YEARS: &str = "(define y (table {:city [\"Oslo\" \"Oslo\" \"Rome\" \"Rome\"] :year [2020 2021 2020 2021] :units [12 30 7 9]}))";

#[test]
fn layers_share_one_plot_and_a_right_axis() {
    let svg = eval(&format!(
        "{SHOP} (to-svg (-> (bars s :city :units) (lines s :city :target {{:dash true}}) (points s :city :share {{:axis :right}}) (hline 10 \"goal\") (title \"Units\" \"by city\") (caption \"source: shop\")))"
    ));
    assert_eq!(svg.matches("<rect").count(), 6, "three bars and three legend swatches: {svg}");
    assert_eq!(svg.matches("<polyline").count(), 1);
    assert_eq!(svg.matches("<circle").count(), 3);
    assert_eq!(svg.matches("stroke-dasharray").count(), 2);
    for needle in [">goal<", ">Units<", ">by city<", ">source: shop<", ">share<", ">units<", ">target<"] {
        assert!(svg.contains(needle), "{needle} missing in {svg}");
    }
    let text = eval(&format!("{SHOP} (str (-> (bars s :city :units) (lines s :city :target)))"));
    assert!(text.starts_with("Oslo ") && text.contains("+----"), "{text}");
    let described = eval(&format!("{SHOP} (plot? (-> (bars s :city :units) (lines s :city :target)))"));
    assert_eq!(described, "true");
    assert!(eval("(-> (pie [\"a\"] [1]) (lines [1] [1]))").contains("cannot share"));
    assert!(eval(&format!("{YEARS} (-> (bars y :city :units {{:facet :year}}) (lines [1] [1]))")).contains("before :facet"));
}

#[test]
fn statistics_add_layers_of_their_own() {
    let trend = eval("(to-svg (-> (points [1 2 3 4] [2 4 6 8]) (trend)))");
    assert_eq!((trend.matches("<circle").count(), trend.matches("<polyline").count()), (4, 1), "{trend}");
    assert!(trend.contains("stroke-dasharray"));
    let smooth = eval("(to-svg (-> (lines [1 2 3 4] [1 3 5 7]) (smooth 2)))");
    assert_eq!(smooth.matches("<polyline").count(), 2);
    assert!(smooth.contains(">2-point mean<"), "{smooth}");
    let inline = eval("(to-svg (lines [1 2 3 4] [1 3 5 7] {:trend true}))");
    assert_eq!(inline.matches("<polyline").count(), 2);
    let density = eval("(to-svg (density [1 2 2 3 3 3 4] {:bandwidth 0.5}))");
    assert_eq!((density.matches("<polygon").count(), density.matches("<polyline").count()), (1, 1), "{density}");
    assert!(density.contains(">density<"));
    let grouped = eval(&format!("{YEARS} (to-svg (density y :units {{:by :city}}))"));
    assert_eq!(grouped.matches("<polygon").count(), 2);
    let normalized = eval(&format!("{YEARS} (to-svg (bars y :city :units {{:by :year :normalize true}}))"));
    assert!(normalized.contains(">100%<") && normalized.contains(">0%<"), "{normalized}");
    let steps = eval("(to-svg (steps [1 2 3] [1 2 3]))");
    assert_eq!(steps.matches("<polyline").count(), 1);
    let lollipops = eval("(to-svg (lollipops [\"a\" \"b\"] [1 2]))");
    assert_eq!((lollipops.matches("<circle").count(), lollipops.matches("<rect").count()), (2, 0), "{lollipops}");
    assert_eq!(eval("(smooth (bars [\"a\"] [1]) 0)"), "error: smooth expects a positive window, got 0");
}

#[test]
fn encodings_palettes_and_category_order() {
    let sized = eval(&format!("{SHOP} (to-svg (points s :units :target {{:size :share}}))"));
    let radii: std::collections::BTreeSet<&str> = sized.split("r=\"").skip(1).map(|rest| rest.split('"').next().unwrap_or("")).collect();
    assert!(radii.len() >= 2, "{sized}");
    let shaped = eval(&format!("{YEARS} (to-svg (points y :year :units {{:by :city :shapes true}}))"));
    assert!(shaped.contains("<circle") && shaped.contains("<polygon"), "{shaped}");
    let sorted = eval("(str (bars [\"a\" \"b\" \"c\"] [1 3 2] {:sort :desc}))");
    let lines: Vec<&str> = sorted.lines().collect();
    assert!(lines[0].starts_with("b ") && lines[1].starts_with("c ") && lines[2].starts_with("a "), "{sorted}");
    let topped = eval("(str (bars [\"a\" \"b\" \"c\" \"d\"] [5 4 1 2] {:top 2}))");
    assert!(topped.lines().last().is_some_and(|l| l.starts_with("Other") && l.ends_with(" 3")), "{topped}");
    assert!(eval("(to-svg (bars [\"a\"] [1] {:palette [\"#112233\"]}))").contains("fill=\"#112233\""));
    assert!(eval("(to-svg (-> (bars [\"a\"] [1]) (palette :warm)))").contains("#b4540f"));
    let colored = eval("(to-svg (lines [1 2] [1 2] {:color \"#abcdef\" :width 4 :opacity 0.5}))");
    assert!(colored.contains("#abcdef") && colored.contains("stroke-width=\"4\""), "{colored}");
    let right = eval(&format!("{YEARS} (to-svg (bars y :city :units {{:by :year :legend :right}}))"));
    assert_eq!(right.matches("<rect").count(), 6, "{right}");
    let hidden = eval(&format!("{YEARS} (to-svg (bars y :city :units {{:by :year :legend :none}}))"));
    assert_eq!(hidden.matches("<rect").count(), 4, "{hidden}");
    assert!(eval("(to-svg (bars [\"a\"] [2500000] {:y-format :compact}))").contains("M<"));
    let long = eval("(to-svg (bars [\"a very long category name\" \"another quite long name\" \"third long label\" \"fourth long label here\"] [1 2 3 4]))");
    assert!(long.contains("rotate(-40"), "{long}");
    let annotated = eval("(to-svg (-> (lines [1 2 3] [1 4 9]) (annotate 2 4 \"peak\") (vline 2 \"mid\") (x-axis {:label \"step\" :ticks 3}) (y-axis {:format \"{:.1}\"})))");
    assert!(annotated.contains(">peak<") && annotated.contains(">mid<") && annotated.contains(">step<") && annotated.contains(">4.0<"), "{annotated}");
    assert!(eval("(legend (bars [\"a\"] [1]) :sideways)").contains(":top, :right"));
    assert_eq!(eval("(bars [\"a\"] [1] {:shape :star})"), "error: bars expects :circle, :square, :diamond, or :triangle for :shape, got :star");
}

#[test]
fn heatmaps_figures_exports_and_themes() {
    let heat = "(define h (table {:x [\"a\" \"b\" \"a\" \"b\"] :y [\"p\" \"p\" \"q\" \"q\"] :v [1 2 3 4]}))";
    let text = eval(&format!("{heat} (str (heatmap h :x :y :v))"));
    assert!(text.contains("q") && text.contains("4"), "{text}");
    let svg = eval(&format!("{heat} (to-svg (heatmap h :x :y :v))"));
    assert_eq!(svg.matches("<rect").count(), 4, "{svg}");
    assert!(eval("(heatmap [1] :a :b :c)").contains("expects a table"));
    let figure = eval("(to-svg (figure [(bars [\"a\"] [1]) (lines [1 2] [1 2])] {:columns 2 :title \"Two\"}))");
    assert!(figure.contains(">Two<") && figure.matches("<rect").count() == 1 && figure.matches("<polyline").count() == 1, "{figure}");
    let sized = eval("(to-svg (bars [\"a\"] [1]) {:width 300 :height 200 :theme :dark})");
    assert!(sized.contains("width=\"300\"") && sized.contains("#161a21"), "{sized}");
    let themed = eval("(plot-theme! {:palette [\"#010203\"] :grid false}) (define out (to-svg (bars [\"a\"] [1]))) (plot-theme! {:palette [] :grid true}) out");
    assert!(themed.contains("#010203") && !themed.contains("#e3e7ee"), "{themed}");
    assert_eq!(eval("(plot-theme)"), "{:palette [], :grid true, :legend :auto}");
    assert_eq!(eval("(bars 5 6)"), "error: bars expects a table or a collection, got 5");
}
