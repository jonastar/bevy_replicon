mod common;

use bevy::{ecs::event::Events, prelude::*, time::TimePlugin};
use bevy_renet::renet::{transport::NetcodeClientTransport, ClientId};
use bevy_replicon::prelude::*;

use common::DummyEvent;

#[test]
fn without_server_plugin() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        ReplicationPlugins.build().disable::<ServerPlugin>(),
    ))
    .add_server_event_with::<DummyEvent, _, _>(EventType::Ordered, || {}, || {})
    .update();
}

#[test]
fn without_client_plugin() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        ReplicationPlugins.build().disable::<ClientPlugin>(),
    ))
    .add_server_event_with::<DummyEvent, _, _>(EventType::Ordered, || {}, || {})
    .update();
}

#[test]
fn sending_receiving() {
    let mut server_app = App::new();
    let mut client_app = App::new();
    for app in [&mut server_app, &mut client_app] {
        app.add_plugins((
            MinimalPlugins,
            ReplicationPlugins.set(ServerPlugin::new(TickPolicy::EveryFrame)),
        ))
        .add_server_event::<DummyEvent>(EventType::Ordered);
    }

    common::connect(&mut server_app, &mut client_app);

    let client_transport = client_app.world.resource::<NetcodeClientTransport>();
    let client_id = ClientId::from_raw(client_transport.client_id());

    for (mode, events_count) in [
        (SendMode::Broadcast, 1),
        (SendMode::Direct(SERVER_ID), 0),
        (SendMode::Direct(client_id), 1),
        (SendMode::BroadcastExcept(SERVER_ID), 1),
        (SendMode::BroadcastExcept(client_id), 0),
    ] {
        server_app
            .world
            .resource_mut::<Events<ToClients<DummyEvent>>>()
            .send(ToClients {
                mode,
                event: DummyEvent(Entity::PLACEHOLDER),
            });

        server_app.update();
        client_app.update();

        let mut dummy_events = client_app.world.resource_mut::<Events<DummyEvent>>();
        assert_eq!(
            dummy_events.drain().count(),
            events_count,
            "event should be emited {events_count} times for {mode:?}"
        );
    }
}

#[test]
fn sending_receiving_and_mapping() {
    let mut server_app = App::new();
    let mut client_app = App::new();
    for app in [&mut server_app, &mut client_app] {
        app.add_plugins((
            MinimalPlugins,
            ReplicationPlugins.set(ServerPlugin::new(TickPolicy::EveryFrame)),
        ))
        .add_mapped_server_event::<DummyEvent>(EventType::Ordered);
    }

    common::connect(&mut server_app, &mut client_app);

    let client_entity = Entity::from_raw(0);
    let server_entity = Entity::from_raw(client_entity.index() + 1);
    client_app
        .world
        .resource_mut::<ServerEntityMap>()
        .insert(server_entity, client_entity);

    server_app
        .world
        .resource_mut::<Events<ToClients<DummyEvent>>>()
        .send(ToClients {
            mode: SendMode::Broadcast,
            event: DummyEvent(server_entity),
        });

    server_app.update();
    client_app.update();

    let mapped_entities: Vec<_> = client_app
        .world
        .resource_mut::<Events<DummyEvent>>()
        .drain()
        .map(|event| event.0)
        .collect();
    assert_eq!(mapped_entities, [client_entity]);
}

#[test]
fn local_resending() {
    let mut app = App::new();
    app.add_plugins((
        TimePlugin,
        ReplicationPlugins.set(ServerPlugin::new(TickPolicy::EveryFrame)),
    ))
    .add_server_event::<DummyEvent>(EventType::Ordered);

    const DUMMY_CLIENT_ID: ClientId = ClientId::from_raw(1);
    for (mode, events_count) in [
        (SendMode::Broadcast, 1),
        (SendMode::Direct(SERVER_ID), 1),
        (SendMode::Direct(DUMMY_CLIENT_ID), 0),
        (SendMode::BroadcastExcept(SERVER_ID), 0),
        (SendMode::BroadcastExcept(DUMMY_CLIENT_ID), 1),
    ] {
        app.world
            .resource_mut::<Events<ToClients<DummyEvent>>>()
            .send(ToClients {
                mode,
                event: DummyEvent(Entity::PLACEHOLDER),
            });

        app.update();

        let server_events = app.world.resource::<Events<ToClients<DummyEvent>>>();
        assert!(server_events.is_empty());

        let mut dummy_events = app.world.resource_mut::<Events<DummyEvent>>();
        assert_eq!(
            dummy_events.drain().count(),
            events_count,
            "event should be emited {events_count} times for {mode:?}"
        );
    }
}

#[test]
fn event_queue() {
    let mut server_app = App::new();
    let mut client_app = App::new();
    for app in [&mut server_app, &mut client_app] {
        app.add_plugins((
            MinimalPlugins,
            ReplicationPlugins.set(ServerPlugin::new(TickPolicy::EveryFrame)),
        ))
        .add_server_event::<DummyEvent>(EventType::Ordered);
    }

    common::connect(&mut server_app, &mut client_app);

    // Simulate event that received two ticks earlier.
    let mut tick = *server_app.world.resource::<RepliconTick>();
    tick.increment_by(2);
    client_app
        .world
        .resource_mut::<ServerEventQueue<DummyEvent>>()
        .insert(tick, DummyEvent(Entity::PLACEHOLDER));

    // Send another event to trigger world update.
    server_app
        .world
        .resource_mut::<Events<ToClients<DummyEvent>>>()
        .send(ToClients {
            mode: SendMode::Broadcast,
            event: DummyEvent(Entity::PLACEHOLDER),
        });

    server_app.update();
    client_app.update();

    let mut dummy_events = client_app.world.resource_mut::<Events<DummyEvent>>();
    assert_eq!(
        dummy_events.drain().count(),
        1,
        "should emit only single event for current tick"
    );

    server_app.update();
    client_app.update();

    let dummy_events = client_app.world.resource::<Events<DummyEvent>>();
    assert_eq!(
        dummy_events.len(),
        1,
        "should emit another event received earlier"
    );
}
