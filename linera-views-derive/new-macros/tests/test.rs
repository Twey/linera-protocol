use proc_macro_playground::View;

struct ViewDefault {
    thing: usize,
    subview: SubviewDefault,
}

struct SubviewDefault {
    subthing: usize,
}

#[derive(View)]
#[view(context = FooContext, default = ViewDefault)]
struct View {
    #[view(skip)] thing: usize,
    #[view(skip, default)] second_thing: usize,
    #[view(skip, default = default.thing)] third_thing: usize,
    #[view(default = default.subview)] subview: Subview,
}

#[derive(View)]
#[view(default = SubviewDefaults)]
struct Subview {
    #[view(default = default.subthing)] thing: usize,
}

fn main() {

}
