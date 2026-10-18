//! Shows how to modify texture assets after spawning.

use hearth::{
    asset::RenderAssetUsages, image::ImageLoaderSettings,
    input::common_conditions::input_just_pressed, prelude::*,
};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, (setup, spawn_text))
        .add_systems(
            Update,
            alter_handle.run_if(input_just_pressed(KeyCode::Space)),
        )
        .add_systems(
            Update,
            alter_asset.run_if(input_just_pressed(KeyCode::Enter)),
        )
        .run();
}

#[derive(Component, Debug)]
enum Picture {
    Normal,
    Logo,
}

impl Picture {
    fn get_texture_path(&self) -> String {
        match self {
            Picture::Normal => "branding/hearth_mark.png".into(),
            Picture::Logo => "branding/hearth_logo_dark.png".into(),
        }
    }

    fn set_next_variant(&mut self) {
        *self = match self {
            Picture::Normal => Picture::Logo,
            Picture::Logo => Picture::Normal,
        }
    }
}

#[derive(Component, Debug)]
struct Left;

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    let picture_left = Picture::Normal;
    let picture_right = Picture::Normal;
    commands.spawn(Camera2d);

    let texture_left = asset_server
        .load_builder()
        .with_settings(
            // `RenderAssetUsages::all()` is already the default, so the line below could be omitted.
            // It's helpful to know it exists, however.
            //
            // `RenderAssetUsages` tell Hearth whether to keep the data around:
            //   - for the GPU (`RenderAssetUsages::RENDER_WORLD`),
            //   - for the CPU (`RenderAssetUsages::MAIN_WORLD`),
            //   - or both.
            // `RENDER_WORLD` is necessary to render the image, `MAIN_WORLD` is necessary to inspect
            // and modify the image (via `ResMut<Assets<Image>>`).
            //
            // Since most games will not need to modify textures at runtime, many developers opt to pass
            // only `RENDER_WORLD`. This is more memory efficient, as we don't need to keep the image in
            // RAM. For this example however, this would not work, as we need to inspect and modify the
            // image at runtime.
            |settings: &mut ImageLoaderSettings| settings.asset_usage = RenderAssetUsages::all(),
        )
        .load(picture_left.get_texture_path());

    commands.spawn((
        Name::new("Picture Left"),
        // This marker component ensures we can easily find either of the Pictures by using With and
        // Without query filters.
        Left,
        Sprite::from_image(texture_left),
        Transform::from_xyz(-200.0, 0.0, 0.0),
        picture_left,
    ));

    commands.spawn((
        Name::new("Picture Right"),
        // In contrast to the above, here we rely on the default `RenderAssetUsages` loader setting
        Sprite::from_image(asset_server.load(picture_right.get_texture_path())),
        Transform::from_xyz(200.0, 0.0, 0.0),
        picture_right,
    ));
}

fn spawn_text(mut commands: Commands) {
    commands.spawn((
        Name::new("Instructions"),
        Text::new(
            "Space: swap the right sprite's image handle\n\
            Return: modify the image Asset of the left sprite, affecting all uses of it",
        ),
        Node {
            position_type: PositionType::Absolute,
            top: px(12),
            left: px(12),
            ..default()
        },
    ));
}

fn alter_handle(
    asset_server: Res<AssetServer>,
    right_picture: Single<(&mut Picture, &mut Sprite), Without<Left>>,
) {
    // Image handles, like other parts of the ECS, can be queried as mutable and modified at
    // runtime. We only spawned one picture without the `Left` marker component.
    let (mut picture, mut sprite) = right_picture.into_inner();

    // Switch to a new Picture variant
    picture.set_next_variant();

    // Modify the handle associated with the Picture on the right side. Note that we will only
    // have to load the same path from storage media once: repeated attempts will re-use the
    // asset.
    sprite.image = asset_server.load(picture.get_texture_path());
}

fn alter_asset(mut images: ResMut<Assets<Image>>, left_picture: Single<&Sprite, With<Left>>) {
    // Obtain a mutable reference to the Image asset.
    let Some(mut image) = images.get_mut(&left_picture.image) else {
        return;
    };

    for pixel in image.data.as_mut().unwrap() {
        // Directly modify the asset data, which will affect all users of this asset. By
        // contrast, mutating the handle (as we did above) affects only one copy. In this case,
        // we'll just invert the colors, by way of demonstration. Notice that both uses of the
        // asset show the change, not just the one on the left.
        *pixel = 255 - *pixel;
    }
}
