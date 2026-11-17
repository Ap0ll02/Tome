mod input;
mod storage;

use std::time::{Duration, Instant};

use gpui::{
    AnyWindowHandle, App, Application, Bounds, Context, Entity, FocusHandle, KeyBinding, Window,
    WindowBounds, WindowOptions, actions, div, prelude::*, px, rgb, size,
};
use input::{FieldEvent, TextField};
use rand::Rng;
use storage::{Campaign, Character, Storage};

actions!(library, [Back]);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Shelf,
    Book(i64),
    Sheet { campaign_id: i64, character_id: i64 },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Prompt {
    Campaign,
    Character,
}

const ABILITIES: [&str; 6] = [
    "strength",
    "dexterity",
    "constitution",
    "intelligence",
    "wisdom",
    "charisma",
];
const SAVE_FIELDS: [&str; 6] = [
    "save_strength",
    "save_dexterity",
    "save_constitution",
    "save_intelligence",
    "save_wisdom",
    "save_charisma",
];
const SKILLS: [(&str, &str, usize); 18] = [
    ("skill_acrobatics", "Acrobatics (Dex)", 1),
    ("skill_animal_handling", "Animal Handling (Wis)", 4),
    ("skill_arcana", "Arcana (Int)", 3),
    ("skill_athletics", "Athletics (Str)", 0),
    ("skill_deception", "Deception (Cha)", 5),
    ("skill_history", "History (Int)", 3),
    ("skill_insight", "Insight (Wis)", 4),
    ("skill_intimidation", "Intimidation (Cha)", 5),
    ("skill_investigation", "Investigation (Int)", 3),
    ("skill_medicine", "Medicine (Wis)", 4),
    ("skill_nature", "Nature (Int)", 3),
    ("skill_perception", "Perception (Wis)", 4),
    ("skill_performance", "Performance (Cha)", 5),
    ("skill_persuasion", "Persuasion (Cha)", 5),
    ("skill_religion", "Religion (Int)", 3),
    ("skill_sleight_of_hand", "Sleight of Hand (Dex)", 1),
    ("skill_stealth", "Stealth (Dex)", 1),
    ("skill_survival", "Survival (Wis)", 4),
];

struct Theme {
    background: u32,
    surface: u32,
    edge: u32,
    text: u32,
    muted: u32,
    accent: u32,
    book: u32,
    book_hover: u32,
}

const DARK: Theme = Theme {
    background: 0x141414,
    surface: 0x1d1c1a,
    edge: 0x37332e,
    text: 0xe9e2d5,
    muted: 0x9f9689,
    accent: 0xc7a778,
    book: 0x39312b,
    book_hover: 0x58483a,
};

struct Library {
    screen: Screen,
    prompt: Option<Prompt>,
    campaigns: Vec<Campaign>,
    characters: Vec<Character>,
    name_field: Entity<TextField>,
    sheet_field: Option<Entity<TextField>>,
    editing: Option<&'static str>,
    focus: FocusHandle,
    window_handle: AnyWindowHandle,
    escape_deadline: Option<Instant>,
    storage: Storage,
    save_error: Option<String>,
}

impl Library {
    fn proficiency_bonus(character: &Character) -> Option<i64> {
        character
            .proficiency_bonus_override
            .or_else(|| character.level.map(|level| 2 + (level - 1).div_euclid(4)))
    }

    fn saving_throw_bonus(character: &Character, index: usize) -> Option<i64> {
        character.saving_throw_overrides[index].or_else(|| {
            character.abilities[index].map(|score| {
                (score - 10).div_euclid(2)
                    + if character.saving_throw_proficiencies[index] {
                        Self::proficiency_bonus(character).unwrap_or(0)
                    } else {
                        0
                    }
            })
        })
    }

    fn skill_bonus(character: &Character, index: usize) -> Option<i64> {
        character.skill_overrides[index].or_else(|| {
            let ability = SKILLS[index].2;
            character.abilities[ability].map(|score| {
                (score - 10).div_euclid(2)
                    + if character.skill_proficiencies[index] {
                        Self::proficiency_bonus(character).unwrap_or(0)
                    } else {
                        0
                    }
            })
        })
    }

    fn subscribe_field(field: &Entity<TextField>, cx: &mut Context<Self>) {
        cx.subscribe(field, |this, _, event, cx| match event {
            FieldEvent::Commit => this.save_name(cx),
            FieldEvent::Cancel => {
                this.prompt = None;
                this.focus_root(cx);
                cx.notify();
            }
        })
        .detach();
    }

    fn focus_root(&self, cx: &mut Context<Self>) {
        let focus = self.focus.clone();
        cx.update_window(self.window_handle, |_, window, _| window.focus(&focus))
            .ok();
    }

    fn open_creation(&mut self, _: &gpui::ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.start_prompt(Prompt::Campaign, window, cx);
    }

    fn open_character_creation(
        &mut self,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_prompt(Prompt::Character, window, cx);
    }

    fn start_prompt(&mut self, prompt: Prompt, window: &mut Window, cx: &mut Context<Self>) {
        self.name_field = match prompt {
            Prompt::Campaign => cx.new(TextField::new),
            Prompt::Character => cx.new(TextField::character_name),
        };
        Self::subscribe_field(&self.name_field, cx);
        self.save_error = None;
        self.prompt = Some(prompt);
        window.focus(self.name_field.read(cx).focus());
        cx.notify();
    }

    fn dismiss_welcome(&mut self, _: &gpui::ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.prompt = None;
        self.focus_root(cx);
        cx.notify();
    }

    fn create_campaign(&mut self, _: &gpui::ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.save_name(cx);
    }

    fn save_name(&mut self, cx: &mut Context<Self>) {
        let name = self.name_field.read(cx).value();
        let result = match (self.prompt, self.screen) {
            (Some(Prompt::Campaign), _) => self.storage.create_campaign(&name).map(|campaign| {
                self.campaigns.push(campaign);
            }),
            (Some(Prompt::Character), Screen::Book(campaign_id)) => self
                .storage
                .create_character(campaign_id, &name)
                .map(|character| {
                    let character_id = character.id;
                    self.characters.push(character);
                    self.screen = Screen::Sheet {
                        campaign_id,
                        character_id,
                    };
                }),
            _ => return,
        };
        match result {
            Ok(()) => {
                self.prompt = None;
                self.save_error = None;
                self.focus_root(cx);
            }
            Err(error) => self.save_error = Some(format!("Could not save: {error}")),
        }
        cx.notify();
    }

    fn click_outside(&mut self, _: &gpui::ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        match self.prompt {
            Some(Prompt::Campaign) => self.save_name(cx),
            Some(Prompt::Character) => {
                self.prompt = None;
                self.focus_root(cx);
                cx.notify();
            }
            None => {}
        }
    }

    fn open_book(&mut self, campaign_id: i64, window: &mut Window, cx: &mut Context<Self>) {
        match self.storage.characters(campaign_id) {
            Ok(characters) => {
                self.characters = characters;
                self.screen = Screen::Book(campaign_id);
                window.focus(&self.focus);
            }
            Err(error) => self.save_error = Some(format!("Could not open campaign: {error}")),
        }
        cx.notify();
    }

    fn back(&mut self, _: &Back, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing.is_some() {
            self.cancel_sheet_edit(window, cx);
            return;
        }
        if self.prompt.is_some() {
            self.prompt = None;
        } else {
            match self.screen {
                Screen::Shelf => return,
                Screen::Book(_) => self.screen = Screen::Shelf,
                Screen::Sheet { campaign_id, .. } => {
                    if self
                        .escape_deadline
                        .is_some_and(|until| Instant::now() <= until)
                    {
                        self.screen = Screen::Book(campaign_id);
                        self.escape_deadline = None;
                    } else {
                        let until = Instant::now() + Duration::from_secs(3);
                        self.escape_deadline = Some(until);
                        let timer = cx.background_executor().timer(Duration::from_secs(3));
                        cx.spawn(async move |this, cx| {
                            timer.await;
                            this.update(cx, |this, cx| {
                                if this.escape_deadline == Some(until) {
                                    this.escape_deadline = None;
                                    cx.notify();
                                }
                            })
                            .ok();
                        })
                        .detach();
                    }
                }
            }
        }
        window.focus(&self.focus);
        cx.notify();
    }

    fn open_sheet(
        &mut self,
        campaign_id: i64,
        character_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.screen = Screen::Sheet {
            campaign_id,
            character_id,
        };
        self.escape_deadline = None;
        window.focus(&self.focus);
        cx.notify();
    }

    fn sheet_value(&self, character_id: i64, field: &str) -> String {
        let Some(character) = self.characters.iter().find(|item| item.id == character_id) else {
            return String::new();
        };
        match field {
            "name" => character.name.clone(),
            "class" => character.class.clone(),
            "level" => character.level.map(|n| n.to_string()).unwrap_or_default(),
            "background" => character.background.clone(),
            "player_name" => character.player_name.clone(),
            "race" => character.race.clone(),
            "alignment" => character.alignment.clone(),
            "experience_points" => character
                .experience_points
                .map(|n| n.to_string())
                .unwrap_or_default(),
            "armor_class" => character
                .armor_class
                .map(|n| n.to_string())
                .unwrap_or_default(),
            "initiative_override" => character
                .initiative_override
                .or_else(|| character.abilities[1].map(|dex| (dex - 10).div_euclid(2)))
                .map(|n| format!("{n:+}"))
                .unwrap_or_default(),
            "speed" => character.speed.clone(),
            "hit_point_maximum" => character
                .hit_point_maximum
                .map(|n| n.to_string())
                .unwrap_or_default(),
            "current_hit_points" => character
                .current_hit_points
                .map(|n| n.to_string())
                .unwrap_or_default(),
            "temporary_hit_points" => character
                .temporary_hit_points
                .map(|n| n.to_string())
                .unwrap_or_default(),
            "hit_dice" => character.hit_dice.clone(),
            "proficiency_bonus_override" => Self::proficiency_bonus(character)
                .map(|n| format!("{n:+}"))
                .unwrap_or_default(),
            "passive_perception_override" => character
                .passive_perception_override
                .or_else(|| Self::skill_bonus(character, 11).map(|bonus| 10 + bonus))
                .map(|n| n.to_string())
                .unwrap_or_default(),
            "other_proficiencies" => character.other_proficiencies.clone(),
            field if SAVE_FIELDS.contains(&field) => {
                let index = SAVE_FIELDS.iter().position(|save| *save == field).unwrap();
                Self::saving_throw_bonus(character, index)
                    .map(|n| format!("{n:+}"))
                    .unwrap_or_default()
            }
            field if SKILLS.iter().any(|skill| skill.0 == field) => {
                let index = SKILLS.iter().position(|skill| skill.0 == field).unwrap();
                Self::skill_bonus(character, index)
                    .map(|n| format!("{n:+}"))
                    .unwrap_or_default()
            }
            _ => ABILITIES
                .iter()
                .position(|ability| *ability == field)
                .and_then(|i| character.abilities[i])
                .map(|n| n.to_string())
                .unwrap_or_default(),
        }
    }

    fn start_sheet_edit(
        &mut self,
        field: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_sheet_edit(cx);
        if self.editing.is_some() {
            return;
        }
        let Screen::Sheet { character_id, .. } = self.screen else {
            return;
        };
        let character = self.characters.iter().find(|c| c.id == character_id);
        let override_value = match field {
            "initiative_override" => character.and_then(|c| c.initiative_override),
            "proficiency_bonus_override" => character.and_then(|c| c.proficiency_bonus_override),
            "passive_perception_override" => character.and_then(|c| c.passive_perception_override),
            field if SAVE_FIELDS.contains(&field) => character.and_then(|c| {
                let index = SAVE_FIELDS.iter().position(|save| *save == field)?;
                c.saving_throw_overrides[index]
            }),
            field if SKILLS.iter().any(|skill| skill.0 == field) => {
                let index = SKILLS.iter().position(|skill| skill.0 == field).unwrap();
                character.and_then(|c| c.skill_overrides[index])
            }
            _ => None,
        };
        let value = if field == "initiative_override"
            || field == "proficiency_bonus_override"
            || field == "passive_perception_override"
            || SAVE_FIELDS.contains(&field)
            || SKILLS.iter().any(|skill| skill.0 == field)
        {
            override_value.map(|n| format!("{n:+}")).unwrap_or_default()
        } else {
            self.sheet_value(character_id, field)
        };
        let input = cx.new(|cx| {
            let input = TextField::with_value(cx, value, field);
            if field == "other_proficiencies" {
                input.multiline()
            } else if ABILITIES.contains(&field)
                || SAVE_FIELDS.contains(&field)
                || SKILLS.iter().any(|skill| skill.0 == field)
            {
                input.compact()
            } else {
                input
            }
        });
        cx.subscribe(&input, |this, _, event, cx| match event {
            FieldEvent::Commit => {
                this.save_sheet_edit(cx);
                this.focus_root(cx);
            }
            FieldEvent::Cancel => {
                this.editing = None;
                this.sheet_field = None;
                this.focus_root(cx);
                cx.notify();
            }
        })
        .detach();
        window.focus(input.read(cx).focus());
        self.sheet_field = Some(input);
        self.editing = Some(field);
        cx.notify();
    }

    fn save_sheet_edit(&mut self, cx: &mut Context<Self>) {
        let (Some(field), Some(input), Screen::Sheet { character_id, .. }) =
            (self.editing, self.sheet_field.as_ref(), self.screen)
        else {
            return;
        };
        let value = input.read(cx).value();
        let result = if let Some(index) = SAVE_FIELDS.iter().position(|save| *save == field) {
            let override_bonus = if value.trim().is_empty() {
                Ok(None)
            } else {
                value.trim().parse::<i64>().map(Some).map_err(Into::into)
            };
            override_bonus.and_then(|bonus| {
                let proficient = self
                    .characters
                    .iter()
                    .find(|c| c.id == character_id)
                    .is_some_and(|c| c.saving_throw_proficiencies[index]);
                self.storage
                    .save_saving_throw(character_id, index, proficient, bonus)
            })
        } else if let Some(index) = SKILLS.iter().position(|skill| skill.0 == field) {
            let override_bonus = if value.trim().is_empty() {
                Ok(None)
            } else {
                value.trim().parse::<i64>().map(Some).map_err(Into::into)
            };
            override_bonus.and_then(|bonus| {
                let proficient = self
                    .characters
                    .iter()
                    .find(|c| c.id == character_id)
                    .is_some_and(|c| c.skill_proficiencies[index]);
                self.storage
                    .save_skill(character_id, index, proficient, bonus)
            })
        } else {
            self.storage.save_field(character_id, field, &value)
        };
        match result {
            Ok(()) => {
                if let Some(character) = self.characters.iter_mut().find(|c| c.id == character_id) {
                    match field {
                        "name" => character.name = value,
                        "class" => character.class = value,
                        "level" => character.level = value.trim().parse().ok(),
                        "background" => character.background = value,
                        "player_name" => character.player_name = value,
                        "race" => character.race = value,
                        "alignment" => character.alignment = value,
                        "experience_points" => {
                            character.experience_points = value.trim().parse().ok()
                        }
                        "armor_class" => character.armor_class = value.trim().parse().ok(),
                        "initiative_override" => {
                            character.initiative_override = value.trim().parse().ok()
                        }
                        "speed" => character.speed = value,
                        "hit_point_maximum" => {
                            character.hit_point_maximum = value.trim().parse().ok()
                        }
                        "current_hit_points" => {
                            character.current_hit_points = value.trim().parse().ok()
                        }
                        "temporary_hit_points" => {
                            character.temporary_hit_points = value.trim().parse().ok()
                        }
                        "hit_dice" => character.hit_dice = value,
                        "proficiency_bonus_override" => {
                            character.proficiency_bonus_override = value.trim().parse().ok()
                        }
                        "passive_perception_override" => {
                            character.passive_perception_override = value.trim().parse().ok()
                        }
                        "other_proficiencies" => character.other_proficiencies = value,
                        field if SAVE_FIELDS.contains(&field) => {
                            let index = SAVE_FIELDS.iter().position(|save| *save == field).unwrap();
                            character.saving_throw_overrides[index] = value.trim().parse().ok();
                        }
                        field if SKILLS.iter().any(|skill| skill.0 == field) => {
                            let index = SKILLS.iter().position(|skill| skill.0 == field).unwrap();
                            character.skill_overrides[index] = value.trim().parse().ok();
                        }
                        _ => {
                            if let Some(i) = ABILITIES.iter().position(|ability| *ability == field)
                            {
                                character.abilities[i] = value.trim().parse().ok();
                            }
                        }
                    }
                }
                self.editing = None;
                self.sheet_field = None;
                self.save_error = None;
            }
            Err(error) => self.save_error = Some(format!("Could not save {field}: {error}")),
        }
        cx.notify();
    }

    fn cancel_sheet_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editing = None;
        self.sheet_field = None;
        window.focus(&self.focus);
        cx.notify();
    }

    fn roll_ability(&mut self, field: &'static str, cx: &mut Context<Self>) {
        let Screen::Sheet { character_id, .. } = self.screen else {
            return;
        };
        if self.editing != Some(field)
            || !self.sheet_value(character_id, field).is_empty()
            || self
                .sheet_field
                .as_ref()
                .is_none_or(|input| !input.read(cx).value().trim().is_empty())
        {
            return;
        }

        let mut dice = [0_i64; 4];
        let mut rng = rand::rng();
        for die in &mut dice {
            *die = rng.random_range(1..=6);
        }
        let score = dice.iter().sum::<i64>() - dice.iter().min().unwrap();
        match self
            .storage
            .save_field(character_id, field, &score.to_string())
        {
            Ok(()) => {
                if let Some(character) = self.characters.iter_mut().find(|c| c.id == character_id) {
                    let index = ABILITIES
                        .iter()
                        .position(|ability| *ability == field)
                        .unwrap();
                    character.abilities[index] = Some(score);
                }
                self.editing = None;
                self.sheet_field = None;
                self.save_error = None;
                self.focus_root(cx);
            }
            Err(error) => self.save_error = Some(format!("Could not save {field}: {error}")),
        }
        cx.notify();
    }

    fn death_save_row(
        &self,
        character_id: i64,
        field: &'static str,
        label: &'static str,
        count: i64,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(DARK.muted))
                    .child(label),
            )
            .child(div().flex().gap_1().children((1..=3).map(move |mark| {
                div()
                    .id((field, mark as usize))
                    .cursor_pointer()
                    .text_size(px(20.))
                    .text_color(rgb(DARK.accent))
                    .child(if mark <= count { "●" } else { "○" })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        let next = if count >= mark { mark - 1 } else { mark };
                        match this
                            .storage
                            .save_field(character_id, field, &next.to_string())
                        {
                            Ok(()) => {
                                if let Some(character) =
                                    this.characters.iter_mut().find(|c| c.id == character_id)
                                {
                                    if field == "death_successes" {
                                        character.death_successes = next;
                                    } else {
                                        character.death_failures = next;
                                    }
                                }
                                this.save_error = None;
                            }
                            Err(error) => {
                                this.save_error =
                                    Some(format!("Could not save death saves: {error}"))
                            }
                        }
                        cx.notify();
                    }))
            })))
    }

    fn toggle_inspiration(&mut self, character_id: i64, cx: &mut Context<Self>) {
        self.save_sheet_edit(cx);
        if self.editing.is_some() {
            return;
        }
        let Some(character) = self.characters.iter_mut().find(|c| c.id == character_id) else {
            return;
        };
        let inspired = !character.inspiration;
        match self.storage.save_inspiration(character_id, inspired) {
            Ok(()) => {
                character.inspiration = inspired;
                self.save_error = None;
            }
            Err(error) => self.save_error = Some(format!("Could not save inspiration: {error}")),
        }
        cx.notify();
    }

    fn toggle_saving_throw(&mut self, character_id: i64, index: usize, cx: &mut Context<Self>) {
        self.save_sheet_edit(cx);
        if self.editing.is_some() {
            return;
        }
        let Some(character) = self.characters.iter_mut().find(|c| c.id == character_id) else {
            return;
        };
        let proficient = !character.saving_throw_proficiencies[index];
        match self.storage.save_saving_throw(
            character_id,
            index,
            proficient,
            character.saving_throw_overrides[index],
        ) {
            Ok(()) => {
                character.saving_throw_proficiencies[index] = proficient;
                self.save_error = None;
            }
            Err(error) => self.save_error = Some(format!("Could not save saving throw: {error}")),
        }
        cx.notify();
    }

    fn toggle_skill(&mut self, character_id: i64, index: usize, cx: &mut Context<Self>) {
        self.save_sheet_edit(cx);
        if self.editing.is_some() {
            return;
        }
        let Some(character) = self.characters.iter_mut().find(|c| c.id == character_id) else {
            return;
        };
        let proficient = !character.skill_proficiencies[index];
        match self.storage.save_skill(
            character_id,
            index,
            proficient,
            character.skill_overrides[index],
        ) {
            Ok(()) => {
                character.skill_proficiencies[index] = proficient;
                self.save_error = None;
            }
            Err(error) => self.save_error = Some(format!("Could not save skill: {error}")),
        }
        cx.notify();
    }

    fn skill_row(
        &self,
        character_id: i64,
        index: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (field, label, _) = SKILLS[index];
        let proficient = self
            .characters
            .iter()
            .find(|c| c.id == character_id)
            .is_some_and(|c| c.skill_proficiencies[index]);
        let value = self.sheet_value(character_id, field);
        div()
            .flex()
            .items_center()
            .gap_1()
            .child(
                div()
                    .id(("skill-proficiency", index))
                    .cursor_pointer()
                    .text_size(px(18.))
                    .text_color(rgb(DARK.accent))
                    .child(if proficient { "●" } else { "○" })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.toggle_skill(character_id, index, cx);
                    })),
            )
            .child(
                div()
                    .id(("skill-value", index))
                    .w(px(52.))
                    .cursor_pointer()
                    .child(if self.editing == Some(field) {
                        self.sheet_field
                            .as_ref()
                            .unwrap()
                            .clone()
                            .into_any_element()
                    } else {
                        div()
                            .text_size(px(13.))
                            .child(if value.is_empty() {
                                "—".into()
                            } else {
                                value
                            })
                            .into_any_element()
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        if this.editing != Some(field) {
                            this.start_sheet_edit(field, window, cx);
                        }
                    })),
            )
            .child(div().text_size(px(10.)).child(label))
    }

    fn saving_throw_row(
        &self,
        character_id: i64,
        index: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let field = SAVE_FIELDS[index];
        let character = self.characters.iter().find(|c| c.id == character_id);
        let proficient = character.is_some_and(|c| c.saving_throw_proficiencies[index]);
        let value = self.sheet_value(character_id, field);
        div()
            .flex()
            .items_center()
            .gap_1()
            .child(
                div()
                    .id(("save-proficiency", index))
                    .cursor_pointer()
                    .text_size(px(18.))
                    .text_color(rgb(DARK.accent))
                    .child(if proficient { "●" } else { "○" })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.toggle_saving_throw(character_id, index, cx);
                    })),
            )
            .child(
                div()
                    .id(("save-value", index))
                    .w(px(50.))
                    .cursor_pointer()
                    .child(if self.editing == Some(field) {
                        self.sheet_field
                            .as_ref()
                            .unwrap()
                            .clone()
                            .into_any_element()
                    } else {
                        div()
                            .text_size(px(14.))
                            .child(if value.is_empty() {
                                "—".into()
                            } else {
                                value
                            })
                            .into_any_element()
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        if this.editing != Some(field) {
                            this.start_sheet_edit(field, window, cx);
                        }
                    })),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .child(ABILITIES[index].to_uppercase()),
            )
    }

    fn sheet_cell(
        &self,
        character_id: i64,
        field: &'static str,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let editing = self.editing == Some(field);
        let value = self.sheet_value(character_id, field);
        let display = if value.is_empty() {
            "—".to_owned()
        } else {
            value
        };
        div()
            .id(field)
            .flex_1()
            .min_w_0()
            .p_3()
            .border_1()
            .border_color(rgb(DARK.edge))
            .cursor_pointer()
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(DARK.muted))
                    .child(label),
            )
            .child(if editing {
                self.sheet_field
                    .as_ref()
                    .unwrap()
                    .clone()
                    .into_any_element()
            } else {
                div()
                    .mt_2()
                    .text_size(px(21.))
                    .child(display)
                    .into_any_element()
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                if this.editing != Some(field) {
                    this.start_sheet_edit(field, window, cx);
                }
            }))
    }

    fn book_view(&self, campaign_id: i64, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &DARK;
        let title = self
            .campaigns
            .iter()
            .find(|campaign| campaign.id == campaign_id)
            .map(|campaign| campaign.name.as_str())
            .unwrap_or("");
        div()
            .size_full()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.text))
            .font_family("IBM Plex Sans")
            .px(px(64.))
            .pt(px(48.))
            .pb(px(40.))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(theme.accent))
                    .child("CAMPAIGN"),
            )
            .child(
                div().mt(px(8.)).text_size(px(34.)).child(
                    if title.is_empty() {
                        "Untitled campaign"
                    } else {
                        title
                    }
                    .to_owned(),
                ),
            )
            .child(
                div()
                    .mt(px(26.))
                    .w_full()
                    .h(px(430.))
                    .flex()
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .p(px(34.))
                            .bg(rgb(theme.surface))
                            .border_1()
                            .border_color(rgb(theme.edge))
                            .children(self.characters.iter().map(|character| {
                                let id = character.id;
                                let name = if character.name.is_empty() {
                                    ""
                                } else {
                                    &character.name
                                };
                                div()
                                    .id(("character", id as u64))
                                    .py_3()
                                    .text_size(px(20.))
                                    .cursor_pointer()
                                    .hover(|row| row.text_color(rgb(theme.accent)))
                                    .child(format!("{}. {}", character.position, name))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.open_sheet(campaign_id, id, window, cx)
                                    }))
                            })),
                    )
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .bg(rgb(theme.surface))
                            .border_1()
                            .border_color(rgb(theme.edge)),
                    ),
            )
            .child(
                div()
                    .id("add-character")
                    .mt(px(22.))
                    .w(px(40.))
                    .h(px(40.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .border_1()
                    .border_color(rgb(theme.edge))
                    .text_size(px(28.))
                    .cursor_pointer()
                    .hover(|button| button.bg(rgb(theme.surface)))
                    .child("+")
                    .on_click(cx.listener(Self::open_character_creation)),
            )
            .when(self.prompt == Some(Prompt::Character), |book| {
                book.child(self.character_prompt(cx))
            })
    }

    fn character_prompt(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &DARK;
        div()
            .id("character-prompt-overlay")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x000000a0))
            .on_click(cx.listener(Self::click_outside))
            .child(
                div()
                    .id("character-prompt")
                    .w(px(390.))
                    .p(px(32.))
                    .flex()
                    .flex_col()
                    .gap_5()
                    .bg(rgb(theme.surface))
                    .border_1()
                    .border_color(rgb(theme.edge))
                    .rounded_md()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(theme.accent))
                            .child("NEW CHARACTER"),
                    )
                    .child(div().text_size(px(28.)).child("Name this sheet"))
                    .child(self.name_field.clone())
                    .when_some(self.save_error.clone(), |card, error| {
                        card.child(
                            div()
                                .text_size(px(13.))
                                .text_color(rgb(0xe8998c))
                                .child(error),
                        )
                    })
                    .child(
                        div()
                            .id("create-character")
                            .px_4()
                            .py_2()
                            .bg(rgb(theme.accent))
                            .text_color(rgb(theme.background))
                            .cursor_pointer()
                            .child("Create character")
                            .on_click(cx.listener(Self::create_campaign)),
                    ),
            )
    }

    fn sheet_view(&self, character_id: i64, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &DARK;
        let (successes, failures) = self
            .characters
            .iter()
            .find(|character| character.id == character_id)
            .map(|character| (character.death_successes, character.death_failures))
            .unwrap_or_default();
        div()
            .id("sheet-background")
            .size_full()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.text))
            .font_family("IBM Plex Sans")
            .flex()
            .items_center()
            .justify_center()
            .on_click(cx.listener(|this, _, _, cx| {
                if this.editing.is_some() {
                    this.save_sheet_edit(cx);
                    this.focus_root(cx);
                }
            }))
            .child(
                div()
                    .id("sheet-page")
                    .w_full()
                    .max_w(px(800.))
                    .h_full()
                    .max_h(px(960.))
                    .overflow_y_scroll()
                    .m_4()
                    .p_6()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .bg(rgb(theme.surface))
                    .border_1()
                    .border_color(rgb(theme.edge))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(theme.accent))
                            .child("CHARACTER SHEET"),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .child(div().w(px(230.)).h(px(84.)).flex().child(self.sheet_cell(
                                character_id,
                                "name",
                                "CHARACTER NAME",
                                cx,
                            )))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .flex()
                                                    .gap_1()
                                                    .child(div().w(px(110.)).flex().child(
                                                        self.sheet_cell(
                                                            character_id,
                                                            "class",
                                                            "CLASS",
                                                            cx,
                                                        ),
                                                    ))
                                                    .child(div().w(px(68.)).flex().child(
                                                        self.sheet_cell(
                                                            character_id,
                                                            "level",
                                                            "LEVEL",
                                                            cx,
                                                        ),
                                                    )),
                                            )
                                            .child(self.sheet_cell(
                                                character_id,
                                                "background",
                                                "BACKGROUND",
                                                cx,
                                            ))
                                            .child(self.sheet_cell(
                                                character_id,
                                                "player_name",
                                                "PLAYER NAME",
                                                cx,
                                            )),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .gap_2()
                                            .child(self.sheet_cell(
                                                character_id,
                                                "race",
                                                "RACE",
                                                cx,
                                            ))
                                            .child(self.sheet_cell(
                                                character_id,
                                                "alignment",
                                                "ALIGNMENT",
                                                cx,
                                            ))
                                            .child(self.sheet_cell(
                                                character_id,
                                                "experience_points",
                                                "EXPERIENCE POINTS",
                                                cx,
                                            )),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .flex_none()
                            .items_start()
                            .child(
                                div()
                                    .w(px(180.))
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(div().flex().flex_col().gap_1().children(
                                        ABILITIES.iter().enumerate().map(|(i, field)| {
                                            let score = self
                                                .characters
                                                .iter()
                                                .find(|c| c.id == character_id)
                                                .and_then(|c| c.abilities[i]);
                                            let modifier = score
                                                .map(|n| format!("{:+}", (n - 10).div_euclid(2)))
                                                .unwrap_or_else(|| "—".to_owned());
                                            div()
                                                .id(*field)
                                                .h(px(84.))
                                                .border_1()
                                                .border_color(rgb(theme.edge))
                                                .p_2()
                                                .flex()
                                                .flex_col()
                                                .justify_center()
                                                .items_center()
                                                .cursor_pointer()
                                                .child(
                                                    div()
                                                        .text_size(px(10.))
                                                        .text_color(rgb(theme.muted))
                                                        .child(field.to_uppercase()),
                                                )
                                                .when(
                                                    score.is_none()
                                                        && self.editing == Some(*field)
                                                        && self.sheet_field.as_ref().is_some_and(
                                                            |input| {
                                                                input
                                                                    .read(cx)
                                                                    .value()
                                                                    .trim()
                                                                    .is_empty()
                                                            },
                                                        ),
                                                    |cell| {
                                                        cell.child(
                                                            div()
                                                                .id(("roll", i))
                                                                .mb_1()
                                                                .px_2()
                                                                .border_1()
                                                                .border_color(rgb(theme.accent))
                                                                .text_color(rgb(theme.accent))
                                                                .text_size(px(12.))
                                                                .cursor_pointer()
                                                                .child("⚄ Roll 4d6")
                                                                .on_click(cx.listener(
                                                                    move |this, _, _, cx| {
                                                                        cx.stop_propagation();
                                                                        this.roll_ability(
                                                                            field, cx,
                                                                        );
                                                                    },
                                                                )),
                                                        )
                                                    },
                                                )
                                                .when(
                                                    score.is_some() || self.editing != Some(*field),
                                                    |cell| {
                                                        cell.child(
                                                            div()
                                                                .text_size(px(22.))
                                                                .child(modifier),
                                                        )
                                                    },
                                                )
                                                .child(if self.editing == Some(*field) {
                                                    div()
                                                        .w(px(72.))
                                                        .child(
                                                            self.sheet_field
                                                                .as_ref()
                                                                .unwrap()
                                                                .clone(),
                                                        )
                                                        .into_any_element()
                                                } else {
                                                    div()
                                                        .text_size(px(15.))
                                                        .child(
                                                            score
                                                                .map(|n| n.to_string())
                                                                .unwrap_or_else(|| "—".into()),
                                                        )
                                                        .into_any_element()
                                                })
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        cx.stop_propagation();
                                                        if this.editing != Some(*field) {
                                                            this.start_sheet_edit(
                                                                field, window, cx,
                                                            );
                                                        }
                                                    },
                                                ))
                                        }),
                                    ))
                                    .child(self.sheet_cell(
                                        character_id,
                                        "passive_perception_override",
                                        "PASSIVE PERCEPTION",
                                        cx,
                                    ))
                                    .child(
                                        div()
                                            .id("other-proficiencies")
                                            .min_h(px(190.))
                                            .p_2()
                                            .border_1()
                                            .border_color(rgb(theme.edge))
                                            .cursor_pointer()
                                            .flex()
                                            .flex_col()
                                            .gap_2()
                                            .child(if self.editing == Some("other_proficiencies") {
                                                self.sheet_field
                                                    .as_ref()
                                                    .unwrap()
                                                    .clone()
                                                    .into_any_element()
                                            } else {
                                                div()
                                                    .text_size(px(13.))
                                                    .child(self.sheet_value(
                                                        character_id,
                                                        "other_proficiencies",
                                                    ))
                                                    .into_any_element()
                                            })
                                            .child(
                                                div()
                                                    .text_size(px(10.))
                                                    .text_color(rgb(theme.muted))
                                                    .child("OTHER PROFICIENCIES & LANGUAGES"),
                                            )
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                cx.stop_propagation();
                                                if this.editing != Some("other_proficiencies") {
                                                    this.start_sheet_edit(
                                                        "other_proficiencies",
                                                        window,
                                                        cx,
                                                    );
                                                }
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .w(px(205.))
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id("inspiration")
                                            .p_2()
                                            .border_1()
                                            .border_color(rgb(theme.edge))
                                            .cursor_pointer()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .text_size(px(20.))
                                                    .text_color(rgb(theme.accent))
                                                    .child(
                                                        if self
                                                            .characters
                                                            .iter()
                                                            .find(|c| c.id == character_id)
                                                            .is_some_and(|c| c.inspiration)
                                                        {
                                                            "●"
                                                        } else {
                                                            "○"
                                                        },
                                                    ),
                                            )
                                            .child(div().text_size(px(12.)).child("INSPIRATION"))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                cx.stop_propagation();
                                                this.toggle_inspiration(character_id, cx);
                                            })),
                                    )
                                    .child(div().h(px(84.)).flex().child(self.sheet_cell(
                                        character_id,
                                        "proficiency_bonus_override",
                                        "PROFICIENCY BONUS",
                                        cx,
                                    )))
                                    .child(
                                        div()
                                            .p_2()
                                            .border_1()
                                            .border_color(rgb(theme.edge))
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .children(
                                                (0..6)
                                                    .map(|index| {
                                                        self.saving_throw_row(
                                                            character_id,
                                                            index,
                                                            cx,
                                                        )
                                                        .into_any_element()
                                                    })
                                                    .collect::<Vec<_>>(),
                                            )
                                            .child(
                                                div()
                                                    .mt_1()
                                                    .text_size(px(11.))
                                                    .text_color(rgb(theme.muted))
                                                    .child("SAVING THROWS"),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .p_2()
                                            .border_1()
                                            .border_color(rgb(theme.edge))
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .children(
                                                (0..SKILLS.len())
                                                    .map(|index| {
                                                        self.skill_row(character_id, index, cx)
                                                            .into_any_element()
                                                    })
                                                    .collect::<Vec<_>>(),
                                            )
                                            .child(
                                                div()
                                                    .mt_1()
                                                    .text_size(px(11.))
                                                    .text_color(rgb(theme.muted))
                                                    .child("SKILLS"),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex()
                                            .gap_2()
                                            .child(self.sheet_cell(
                                                character_id,
                                                "armor_class",
                                                "ARMOR CLASS",
                                                cx,
                                            ))
                                            .child(self.sheet_cell(
                                                character_id,
                                                "initiative_override",
                                                "INITIATIVE",
                                                cx,
                                            ))
                                            .child(self.sheet_cell(
                                                character_id,
                                                "speed",
                                                "SPEED",
                                                cx,
                                            )),
                                    )
                                    .child(self.sheet_cell(
                                        character_id,
                                        "hit_point_maximum",
                                        "HIT POINT MAXIMUM",
                                        cx,
                                    ))
                                    .child(self.sheet_cell(
                                        character_id,
                                        "current_hit_points",
                                        "CURRENT HIT POINTS",
                                        cx,
                                    ))
                                    .child(self.sheet_cell(
                                        character_id,
                                        "temporary_hit_points",
                                        "TEMPORARY HIT POINTS",
                                        cx,
                                    ))
                                    .child(
                                        div()
                                            .flex()
                                            .gap_2()
                                            .child(self.sheet_cell(
                                                character_id,
                                                "hit_dice",
                                                "HIT DICE",
                                                cx,
                                            ))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .p_3()
                                                    .border_1()
                                                    .border_color(rgb(theme.edge))
                                                    .flex()
                                                    .flex_col()
                                                    .justify_center()
                                                    .gap_2()
                                                    .child(
                                                        div()
                                                            .text_size(px(11.))
                                                            .text_color(rgb(theme.muted))
                                                            .child("DEATH SAVES"),
                                                    )
                                                    .child(self.death_save_row(
                                                        character_id,
                                                        "death_successes",
                                                        "SUCCESSES",
                                                        successes,
                                                        cx,
                                                    ))
                                                    .child(self.death_save_row(
                                                        character_id,
                                                        "death_failures",
                                                        "FAILURES",
                                                        failures,
                                                        cx,
                                                    )),
                                            ),
                                    ),
                            ),
                    )
                    .when_some(self.save_error.as_ref(), |page, error| {
                        page.child(div().text_color(rgb(0xe8a090)).child(error.clone()))
                    })
                    .when(self.escape_deadline.is_some(), |page| {
                        page.child(
                            div()
                                .mt(px(30.))
                                .text_size(px(13.))
                                .text_color(rgb(theme.accent))
                                .child("Press Escape again to return to the campaign"),
                        )
                    }),
            )
    }
}

impl Render for Library {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &DARK;
        let shelf = div()
            .size_full()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.text))
            .font_family("IBM Plex Sans")
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .px(px(64.))
                    .pt(px(54.))
                    .pb(px(52.))
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .items_center()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(rgb(theme.accent))
                                            .child("THE LIBRARY"),
                                    )
                                    .child(div().text_size(px(40.)).child("Your campaigns")),
                            )
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(rgb(theme.muted))
                                    .child("A place for every story"),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .justify_end()
                            .child(
                                div()
                                    .flex()
                                    .items_end()
                                    .gap_7()
                                    .h(px(260.))
                                    .children(self.campaigns.iter().enumerate().map(
                                        |(index, campaign)| {
                                            let title = if campaign.name.is_empty() {
                                                "Untitled campaign".to_owned()
                                            } else {
                                                campaign.name.clone()
                                            };
                                            div()
                                                .id(("campaign", campaign.id as u64))
                                                .w(px(168.))
                                                .h(px(224.))
                                                .flex()
                                                .flex_col()
                                                .justify_between()
                                                .p(px(18.))
                                                .bg(rgb(theme.book))
                                                .border_1()
                                                .border_color(rgb(theme.accent))
                                                .rounded_sm()
                                                .cursor_pointer()
                                                .hover(|book| {
                                                    book.bg(rgb(theme.book_hover)).h(px(232.))
                                                })
                                                .on_click(cx.listener({
                                                    let id = campaign.id;
                                                    move |this, _, window, cx| {
                                                        this.open_book(id, window, cx)
                                                    }
                                                }))
                                                .child(
                                                    div()
                                                        .text_size(px(11.))
                                                        .text_color(rgb(theme.accent))
                                                        .child(format!(
                                                            "CAMPAIGN {:02}",
                                                            index + 1
                                                        )),
                                                )
                                                .child(div().text_size(px(25.)).child(title))
                                        },
                                    ))
                                    .when(self.campaigns.is_empty(), |shelf| {
                                        shelf.child(
                                            div()
                                                .pb(px(30.))
                                                .text_size(px(15.))
                                                .text_color(rgb(theme.muted))
                                                .child(
                                                    "An empty shelf, ready for a new adventure.",
                                                ),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .h(px(12.))
                                    .bg(rgb(theme.surface))
                                    .border_t_1()
                                    .border_color(rgb(theme.edge)),
                            ),
                    )
                    .child(
                        div()
                            .id("add-campaign")
                            .mt(px(28.))
                            .w(px(40.))
                            .h(px(40.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .border_1()
                            .border_color(rgb(theme.edge))
                            .text_size(px(28.))
                            .cursor_pointer()
                            .hover(|button| button.bg(rgb(theme.surface)))
                            .child("+")
                            .on_click(cx.listener(Self::open_creation)),
                    )
                    .when(self.prompt == Some(Prompt::Campaign), |root| {
                        root.child(
                            div()
                                .id("welcome-overlay")
                                .absolute()
                                .inset_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .bg(gpui::rgba(0x000000a0))
                                .on_click(cx.listener(Self::click_outside))
                                .child(
                                    div()
                                        .id("welcome-card")
                                        .w(px(390.))
                                        .p(px(32.))
                                        .flex()
                                        .flex_col()
                                        .gap_5()
                                        .bg(rgb(theme.surface))
                                        .border_1()
                                        .border_color(rgb(theme.edge))
                                        .rounded_md()
                                        .on_click(|_, _, cx| cx.stop_propagation())
                                        .child(div().child(self.name_field.clone()))
                                        .child(
                                            div()
                                                .text_size(px(11.))
                                                .text_color(rgb(theme.accent))
                                                .child("WELCOME"),
                                        )
                                        .child(div().text_size(px(29.)).child("Begin a campaign"))
                                        .child(
                                            div()
                                                .text_size(px(15.))
                                                .text_color(rgb(theme.muted))
                                                .child(
                                                    "Give your characters a place to call home.",
                                                ),
                                        )
                                        .when_some(self.save_error.clone(), |card, error| {
                                            card.child(
                                                div()
                                                    .text_size(px(13.))
                                                    .text_color(rgb(0xe8998c))
                                                    .child(error),
                                            )
                                        })
                                        .child(
                                            div()
                                                .flex()
                                                .gap_3()
                                                .child(
                                                    div()
                                                        .id("create-campaign")
                                                        .cursor_pointer()
                                                        .px_4()
                                                        .py_2()
                                                        .bg(rgb(theme.accent))
                                                        .text_color(rgb(theme.background))
                                                        .child("Create campaign")
                                                        .on_click(
                                                            cx.listener(Self::create_campaign),
                                                        ),
                                                )
                                                .child(
                                                    div()
                                                        .id("dismiss-welcome")
                                                        .cursor_pointer()
                                                        .px_4()
                                                        .py_2()
                                                        .child("Explore empty shelf")
                                                        .on_click(
                                                            cx.listener(Self::dismiss_welcome),
                                                        ),
                                                ),
                                        ),
                                ),
                        )
                    }),
            );
        let page = match self.screen {
            Screen::Shelf => shelf.into_any_element(),
            Screen::Book(campaign_id) => self.book_view(campaign_id, cx).into_any_element(),
            Screen::Sheet {
                campaign_id: _,
                character_id,
            } => self.sheet_view(character_id, cx).into_any_element(),
        };
        div()
            .size_full()
            .key_context("Library")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::back))
            .child(page)
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("left", input::Left, Some("TextField")),
            KeyBinding::new("right", input::Right, Some("TextField")),
            KeyBinding::new("up", input::Up, Some("TextField")),
            KeyBinding::new("down", input::Down, Some("TextField")),
            KeyBinding::new("shift-left", input::SelectLeft, Some("TextField")),
            KeyBinding::new("shift-right", input::SelectRight, Some("TextField")),
            KeyBinding::new("backspace", input::Backspace, Some("TextField")),
            KeyBinding::new("delete", input::Delete, Some("TextField")),
            KeyBinding::new("cmd-a", input::SelectAll, Some("TextField")),
            KeyBinding::new("enter", input::Commit, Some("TextField")),
            KeyBinding::new("cmd-enter", input::CommitNotes, Some("TextField")),
            KeyBinding::new("escape", input::Cancel, Some("TextField")),
            KeyBinding::new("escape", Back, Some("Library")),
        ]);
        let bounds = Bounds::centered(None, size(px(980.), px(700.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                let storage = Storage::open_default().expect("could not open campaign database");
                let campaigns = storage.campaigns().expect("could not load campaigns");
                let show_welcome = campaigns.is_empty();
                let field = cx.new(TextField::new);
                let focus = cx.focus_handle();
                let window_handle = window.window_handle();
                if show_welcome {
                    window.focus(field.read(cx).focus());
                } else {
                    window.focus(&focus);
                }
                cx.new(|cx| {
                    Library::subscribe_field(&field, cx);
                    Library {
                        screen: Screen::Shelf,
                        prompt: show_welcome.then_some(Prompt::Campaign),
                        campaigns,
                        characters: Vec::new(),
                        name_field: field,
                        sheet_field: None,
                        editing: None,
                        focus,
                        window_handle,
                        escape_deadline: None,
                        storage,
                        save_error: None,
                    }
                })
            },
        )
        .expect("could not open the library window");
        cx.activate(true);
    });
}
