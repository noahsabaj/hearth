use hearth::{
    color::palettes, feathers::theme::ThemeProps, platform::collections::HashMap, prelude::*,
};

/// Creates a basic example theme props for the radio buttons and number inputs.
pub fn basic_example_theme(text_color: Color) -> ThemeProps {
    let mut color = HashMap::new();

    // Radio Button tokens
    color.insert(hearth::feathers::tokens::RADIO_TEXT, text_color);
    color.insert(hearth::feathers::tokens::RADIO_MARK, Color::BLACK);
    color.insert(hearth::feathers::tokens::RADIO_MARK_HOVER, Color::BLACK);
    color.insert(hearth::feathers::tokens::RADIO_MARK_PRESSED, Color::BLACK);

    color.insert(hearth::feathers::tokens::RADIO_BG, Color::WHITE);
    color.insert(
        hearth::feathers::tokens::RADIO_BG_HOVER,
        palettes::basic::GRAY.into(),
    );
    color.insert(
        hearth::feathers::tokens::RADIO_BG_PRESSED,
        palettes::basic::GRAY.into(),
    );
    color.insert(hearth::feathers::tokens::RADIO_BG_CHECKED, Color::WHITE);
    color.insert(
        hearth::feathers::tokens::RADIO_BG_CHECKED_HOVER,
        Color::BLACK,
    );
    color.insert(
        hearth::feathers::tokens::RADIO_BG_CHECKED_PRESSED,
        Color::BLACK,
    );

    color.insert(hearth::feathers::tokens::RADIO_BORDER, Color::BLACK);
    color.insert(
        hearth::feathers::tokens::RADIO_BORDER_HOVER,
        palettes::basic::GRAY.into(),
    );
    color.insert(
        hearth::feathers::tokens::RADIO_BORDER_PRESSED,
        palettes::basic::BLACK.into(),
    );
    color.insert(hearth::feathers::tokens::RADIO_BORDER_CHECKED, Color::BLACK);
    color.insert(
        hearth::feathers::tokens::RADIO_BORDER_CHECKED_HOVER,
        Color::BLACK,
    );
    color.insert(
        hearth::feathers::tokens::RADIO_BORDER_CHECKED_PRESSED,
        Color::BLACK,
    );

    // Number Input tokens
    color.insert(hearth::feathers::tokens::TEXT_INPUT_TEXT, text_color);
    color.insert(hearth::feathers::tokens::TEXT_INPUT_BG, Color::BLACK);
    color.insert(hearth::feathers::tokens::TEXT_INPUT_LABEL_BG, Color::BLACK);

    color.insert(hearth::feathers::tokens::SLIDER_BAR, Color::WHITE);
    color.insert(hearth::feathers::tokens::SLIDER_BAR_HOVER, Color::WHITE);
    color.insert(hearth::feathers::tokens::SLIDER_BAR_PRESSED, Color::WHITE);
    color.insert(hearth::feathers::tokens::SLIDER_BG, Color::BLACK);
    color.insert(hearth::feathers::tokens::SLIDER_BG_HOVER, Color::BLACK);
    color.insert(hearth::feathers::tokens::SLIDER_BG_PRESSED, Color::BLACK);

    color.insert(
        hearth::feathers::tokens::TEXT_INPUT_CURSOR,
        hearth::feathers::palette::ACCENT.lighter(0.2),
    );
    color.insert(
        hearth::feathers::tokens::TEXT_INPUT_SELECTION,
        hearth::feathers::palette::ACCENT,
    );
    color.insert(
        hearth::feathers::tokens::TEXT_INPUT_SELECTION_UNFOCUSED,
        hearth::feathers::palette::TRANSPARENT,
    );

    // Checkbox tokens
    color.insert(hearth::feathers::tokens::CHECKBOX_TEXT, text_color);
    color.insert(hearth::feathers::tokens::CHECKBOX_BG, Color::BLACK);
    color.insert(hearth::feathers::tokens::CHECKBOX_MARK, Color::WHITE);
    color.insert(hearth::feathers::tokens::CHECKBOX_BORDER, Color::BLACK);
    color.insert(
        hearth::feathers::tokens::CHECKBOX_BORDER_HOVER,
        hearth::feathers::palette::GRAY_2,
    );
    color.insert(
        hearth::feathers::tokens::CHECKBOX_BORDER_CHECKED_HOVER,
        hearth::feathers::palette::GRAY_0,
    );
    color.insert(
        hearth::feathers::tokens::CHECKBOX_BORDER_PRESSED,
        hearth::feathers::palette::GRAY_1,
    );
    color.insert(
        hearth::feathers::tokens::CHECKBOX_BORDER_CHECKED_PRESSED,
        hearth::feathers::palette::GRAY_1,
    );
    color.insert(
        hearth::feathers::tokens::CHECKBOX_BORDER_CHECKED,
        Color::BLACK,
    );
    color.insert(
        hearth::feathers::tokens::CHECKBOX_BG_HOVER,
        hearth::feathers::palette::GRAY_0,
    );
    color.insert(hearth::feathers::tokens::CHECKBOX_BG_CHECKED, Color::BLACK);
    color.insert(
        hearth::feathers::tokens::CHECKBOX_BG_CHECKED_HOVER,
        Color::BLACK,
    );
    color.insert(
        hearth::feathers::tokens::CHECKBOX_BG_CHECKED_PRESSED,
        hearth::feathers::palette::GRAY_1,
    );
    color.insert(hearth::feathers::tokens::CHECKBOX_BG_PRESSED, Color::BLACK);
    // Feathers Button / Select
    color.insert(
        hearth::feathers::tokens::BUTTON_BG,
        hearth::feathers::palette::TRANSPARENT,
    );
    color.insert(
        hearth::feathers::tokens::BUTTON_BG_HOVER,
        hearth::feathers::palette::TRANSPARENT,
    );
    color.insert(
        hearth::feathers::tokens::BUTTON_BG_PRESSED,
        hearth::feathers::palette::TRANSPARENT,
    );
    color.insert(
        hearth::feathers::tokens::MENU_BG,
        hearth::feathers::palette::GRAY_0,
    );
    color.insert(
        hearth::feathers::tokens::MENU_BORDER,
        hearth::feathers::palette::GRAY_0,
    );
    color.insert(
        hearth::feathers::tokens::MENUITEM_BG_HOVER,
        hearth::feathers::palette::TRANSPARENT,
    );
    color.insert(
        hearth::feathers::tokens::MENUITEM_BG_PRESSED,
        hearth::feathers::palette::TRANSPARENT,
    );
    color.insert(
        hearth::feathers::tokens::MENUITEM_BG_FOCUSED,
        hearth::feathers::palette::TRANSPARENT,
    );
    color.insert(
        hearth::feathers::tokens::LISTROW_BG,
        hearth::feathers::palette::TRANSPARENT,
    );
    color.insert(
        hearth::feathers::tokens::LISTROW_BG_HOVER,
        hearth::feathers::palette::BLACK,
    );
    color.insert(
        hearth::feathers::tokens::LISTROW_BG_SELECTED,
        hearth::feathers::palette::BLACK,
    );
    color.insert(hearth::feathers::tokens::BUTTON_TEXT, text_color);
    color.insert(hearth::feathers::tokens::LISTROW_TEXT, text_color);

    // Slider tokens
    color.insert(
        hearth::feathers::tokens::SLIDER_BG,
        hearth::feathers::palette::BLACK,
    );
    color.insert(
        hearth::feathers::tokens::SLIDER_BG_HOVER,
        hearth::feathers::palette::GRAY_0,
    );
    color.insert(
        hearth::feathers::tokens::SLIDER_BG_PRESSED,
        hearth::feathers::palette::GRAY_1,
    );
    color.insert(
        hearth::feathers::tokens::SLIDER_BAR,
        hearth::feathers::palette::ACCENT,
    );
    color.insert(
        hearth::feathers::tokens::SLIDER_BAR_HOVER,
        hearth::feathers::palette::ACCENT.lighter(0.05),
    );
    color.insert(
        hearth::feathers::tokens::SLIDER_BAR_PRESSED,
        hearth::feathers::palette::ACCENT.lighter(0.1),
    );
    color.insert(hearth::feathers::tokens::SLIDER_TEXT, text_color);

    // Main text color
    color.insert(hearth::feathers::tokens::TEXT_MAIN, text_color);
    ThemeProps::new_non_contextual(color)
}
